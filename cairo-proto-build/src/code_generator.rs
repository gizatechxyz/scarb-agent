use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::iter;

use itertools::{Either, Itertools};
use log::debug;
use multimap::MultiMap;
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::source_code_info::Location;
use prost_types::{
    DescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FieldOptions, FileDescriptorProto, ServiceDescriptorProto,
    SourceCodeInfo,
};

use crate::ast::{Comments, Method, Service};
use crate::extern_paths::ExternPaths;
use crate::ident::{to_snake, to_upper_camel};
use crate::message_graph::MessageGraph;
use crate::{BytesType, Config, MapType};

#[derive(PartialEq)]
enum Syntax {
    Proto2,
    Proto3,
}

pub struct CodeGenerator<'a> {
    config: &'a mut Config,
    package: String,
    type_path: Vec<String>,
    source_info: Option<SourceCodeInfo>,
    syntax: Syntax,
    message_graph: &'a MessageGraph,
    extern_paths: &'a ExternPaths,
    depth: u8,
    path: Vec<i32>,
    buf: &'a mut String,
}

fn push_indent(buf: &mut String, depth: u8) {
    for _ in 0..depth {
        buf.push_str("    ");
    }
}

impl<'a> CodeGenerator<'a> {
    pub fn generate(
        config: &mut Config,
        message_graph: &MessageGraph,
        extern_paths: &ExternPaths,
        file: FileDescriptorProto,
        buf: &mut String,
    ) {
        let source_info = file.source_code_info.map(|mut s| {
            s.location.retain(|loc| {
                let len = loc.path.len();
                len > 0 && len % 2 == 0
            });
            s.location.sort_by(|a, b| a.path.cmp(&b.path));
            s
        });

        let syntax = match file.syntax.as_ref().map(String::as_str) {
            None | Some("proto2") => Syntax::Proto2,
            Some("proto3") => Syntax::Proto3,
            Some(s) => panic!("unknown syntax: {}", s),
        };

        let mut code_gen = CodeGenerator {
            config,
            package: file.package.unwrap_or_default(),
            type_path: Vec::new(),
            source_info,
            syntax,
            message_graph,
            extern_paths,
            depth: 0,
            path: Vec::new(),
            buf,
        };

        debug!(
            "file: {:?}, package: {:?}",
            file.name.as_ref().unwrap(),
            code_gen.package
        );

        code_gen.append_header();

        code_gen.path.push(4);
        for (idx, message) in file.message_type.into_iter().enumerate() {
            code_gen.path.push(idx as i32);
            code_gen.append_message(message);
            code_gen.path.pop();
        }
        code_gen.path.pop();

        code_gen.path.push(5);
        for (idx, desc) in file.enum_type.into_iter().enumerate() {
            panic!("enums are not supported");
        }
        code_gen.path.pop();

        code_gen.path.push(6);
        for (idx, service) in file.service.into_iter().enumerate() {
            code_gen.path.push(idx as i32);
            code_gen.append_service(service);
            code_gen.path.pop();
        }

        code_gen.append_footer();

        code_gen.path.pop();
    }

    fn append_message(&mut self, message: DescriptorProto) {
        debug!("  message: {:?}", message.name());

        let message_name = message.name().to_string();
        let fq_message_name = format!(
            "{}{}{}{}.{}",
            if self.package.is_empty() && self.type_path.is_empty() {
                ""
            } else {
                "."
            },
            self.package.trim_matches('.'),
            if self.type_path.is_empty() { "" } else { "." },
            self.type_path.join("."),
            message_name,
        );

        // Skip external types.
        if self.extern_paths.resolve_ident(&fq_message_name).is_some() {
            return;
        }

        // Split the nested message types into a vector of normal nested message types, and a map
        // of the map field entry types. The path index of the nested message types is preserved so
        // that comments can be retrieved.
        type NestedTypes = Vec<(DescriptorProto, usize)>;
        type MapTypes = HashMap<String, (FieldDescriptorProto, FieldDescriptorProto)>;
        let (nested_types, map_types): (NestedTypes, MapTypes) = message
            .nested_type
            .into_iter()
            .enumerate()
            .partition_map(|(idx, nested_type)| {
                if nested_type
                    .options
                    .as_ref()
                    .and_then(|options| options.map_entry)
                    .unwrap_or(false)
                {
                    let key = nested_type.field[0].clone();
                    let value = nested_type.field[1].clone();
                    assert_eq!("key", key.name());
                    assert_eq!("value", value.name());

                    let name = format!("{}.{}", &fq_message_name, nested_type.name());
                    Either::Right((name, (key, value)))
                } else {
                    Either::Left((nested_type, idx))
                }
            });

        // Split the fields into a vector of the normal fields, and oneof fields.
        // Path indexes are preserved so that comments can be retrieved.
        type Fields = Vec<(FieldDescriptorProto, usize)>;
        type OneofFields = MultiMap<i32, (FieldDescriptorProto, usize)>;
        let (fields, mut oneof_fields): (Fields, OneofFields) = message
            .field
            .into_iter()
            .enumerate()
            .partition_map(|(idx, field)| {
                if field.proto3_optional.unwrap_or(false) {
                    Either::Left((field, idx))
                } else if let Some(oneof_index) = field.oneof_index {
                    Either::Right((oneof_index, (field, idx)))
                } else {
                    Either::Left((field, idx))
                }
            });

        self.push_indent();
        self.buf.push_str("#[derive(Serde, Drop)]\n");
        self.push_indent();
        self.buf.push_str("struct ");
        self.buf.push_str(&to_upper_camel(&message_name));
        self.buf.push_str(" {\n");

        self.depth += 1;
        self.path.push(2);
        for (field, idx) in fields.clone() {
            self.path.push(idx as i32);
            match field
                .type_name
                .as_ref()
                .and_then(|type_name| map_types.get(type_name))
            {
                Some(&(ref key, ref value)) => {
                    self.append_map_field(&fq_message_name, field, key, value)
                }
                None => self.append_field(&fq_message_name, field),
            }
            self.path.pop();
        }
        self.path.pop();

        self.path.push(8);
        for (idx, oneof) in message.oneof_decl.iter().enumerate() {
            let idx = idx as i32;

            let fields = match oneof_fields.get_vec(&idx) {
                Some(fields) => fields,
                None => continue,
            };

            panic!("oneof fields are not supported");

            // self.path.push(idx);
            // self.append_oneof_field(&message_name, &fq_message_name, oneof, fields);
            // self.path.pop();
        }
        self.path.pop();

        self.depth -= 1;
        self.push_indent();
        self.buf.push_str("}\n");

        if !message.enum_type.is_empty() || !nested_types.is_empty() || !oneof_fields.is_empty() {
            self.push_mod(&message_name);
            self.path.push(3);
            for (nested_type, idx) in nested_types {
                self.path.push(idx as i32);
                self.append_message(nested_type);
                self.path.pop();
            }
            self.path.pop();

            self.path.push(4);
            for (idx, nested_enum) in message.enum_type.into_iter().enumerate() {
                panic!("enums are not supported");

                // self.path.push(idx as i32);
                // self.append_enum(nested_enum);
                // self.path.pop();
            }
            self.path.pop();

            for (idx, oneof) in message.oneof_decl.into_iter().enumerate() {
                let idx = idx as i32;
                // optional fields create a synthetic oneof that we want to skip
                let fields = match oneof_fields.remove(&idx) {
                    Some(fields) => fields,
                    None => continue,
                };

                panic!("oneof messages are not supported");

                // self.append_oneof(&fq_message_name, oneof, idx, fields);
            }

            self.pop_mod();
        }

        let type_name = to_upper_camel(&message_name);
        self.buf.push_str(&format!("impl Sendable{type_name} of Sendable<{type_name}> {{\n"));
        self.buf.push_str(&format!("    fn send(self: @{type_name}) {{\n"));
        self.buf.push_str("        cheatcode::<'oracle_path_push'>(array!['struct'].span());\n");
        for (field, idx) in fields.clone() {
            let name = to_snake(field.name());
            self.buf.push_str(&format!("        cheatcode::<'oracle_key_push'>(array!['{name}'].span());\n"));
            self.buf.push_str(&format!("        self.{name}.send();\n"));
            self.buf.push_str(&format!("        cheatcode::<'oracle_key_pop'>(array!['{name}'].span());\n"));
        }
        self.buf.push_str("        cheatcode::<'oracle_path_pop'>(array!['struct'].span());\n");

        self.buf.push_str("    }\n");
        self.buf.push_str(&format!("    fn recv() -> {type_name} {{\n"));

        self.buf.push_str("        cheatcode::<'oracle_path_push'>(array!['struct'].span());\n");
        for (field, idx) in fields.clone() {
            let name = to_snake(field.name());
            let repeated = field.label == Some(Label::Repeated as i32);
            let optional = self.optional(&field);    
            let mut ty = self.resolve_type(&field, &fq_message_name);
            if repeated {
                ty = format!("Array<{ty}>");
            } else if optional {
                ty = format!("Option<{ty}>");
            }

            self.buf.push_str(&format!("        cheatcode::<'oracle_key_push'>(array!['{name}'].span());\n"));
            self.buf.push_str(&format!("        let {name} = Sendable::<{ty}>::recv();\n"));
            self.buf.push_str(&format!("        cheatcode::<'oracle_key_pop'>(array!['{name}'].span());\n"));
        }
        self.buf.push_str("        cheatcode::<'oracle_path_pop'>(array!['struct'].span());\n");

        let all_fields = fields.iter().map(|f| to_snake(f.0.name())).join(", ");
        self.buf.push_str(&format!("        {type_name} {{ {all_fields} }}\n"));
        self.buf.push_str("    }\n");
        self.buf.push_str("}\n");
    }

    fn append_field(&mut self, fq_message_name: &str, field: FieldDescriptorProto) {
        let type_ = field.r#type();
        let repeated = field.label == Some(Label::Repeated as i32);
        let deprecated = self.deprecated(&field);
        let optional = self.optional(&field);
        let ty = self.resolve_type(&field, fq_message_name);

        let boxed = !repeated
            && ((type_ == Type::Message || type_ == Type::Group)
                && self
                    .message_graph
                    .is_nested(field.type_name(), fq_message_name))
            || (self
                .config
                .boxed
                .get_first_field(&fq_message_name, field.name())
                .is_some());

        debug!(
            "    field: {:?}, type: {:?}, boxed: {}",
            field.name(),
            ty,
            boxed
        );

        if deprecated {
            self.push_indent();
            self.buf.push_str("#[deprecated]\n");
        }

        self.push_indent();
        self.buf.push_str(&to_snake(field.name()));
        self.buf.push_str(": ");

        if repeated {
            self.buf.push_str("Array<");
        } else if optional {
            self.buf.push_str("Option<");
        }
        if boxed {
            panic!("boxed types not supported?");
            // self.buf
            //     .push_str(&format!("{}::alloc::boxed::Box<", prost_path));
        }
        self.buf.push_str(&ty);
        if boxed {
            self.buf.push('>');
        }
        if repeated || optional {
            self.buf.push('>');
        }
        self.buf.push_str(",\n");
    }

    fn append_map_field(
        &mut self,
        fq_message_name: &str,
        field: FieldDescriptorProto,
        key: &FieldDescriptorProto,
        value: &FieldDescriptorProto,
    ) {
        let key_ty = self.resolve_type(key, fq_message_name);
        let value_ty = self.resolve_type(value, fq_message_name);

        debug!(
            "    map field: {:?}, key type: {:?}, value type: {:?}",
            field.name(),
            key_ty,
            value_ty
        );
    }

    fn location(&self) -> Option<&Location> {
        let source_info = self.source_info.as_ref()?;
        let idx = source_info
            .location
            .binary_search_by_key(&&self.path[..], |location| &location.path[..])
            .unwrap();
        Some(&source_info.location[idx])
    }


    fn append_service(&mut self, service: ServiceDescriptorProto) {
        let name = service.name().to_owned();
        debug!("  service: {:?}", name);

        let comments = self
            .location()
            .map(Comments::from_location)
            .unwrap_or_default();

        self.path.push(2);
        let methods = service
            .method
            .into_iter()
            .enumerate()
            .map(|(idx, mut method)| {
                debug!("  method: {:?}", method.name());

                self.path.push(idx as i32);
                let comments = self
                    .location()
                    .map(Comments::from_location)
                    .unwrap_or_default();
                self.path.pop();

                let name = method.name.take().unwrap();
                let input_proto_type = method.input_type.take().unwrap();
                let output_proto_type = method.output_type.take().unwrap();
                let input_type = self.resolve_ident(&input_proto_type);
                let output_type = self.resolve_ident(&output_proto_type);
                let client_streaming = method.client_streaming();
                let server_streaming = method.server_streaming();

                Method {
                    name: to_snake(&name),
                    proto_name: name,
                    comments,
                    input_type,
                    output_type,
                    input_proto_type,
                    output_proto_type,
                    options: method.options.unwrap_or_default(),
                    client_streaming,
                    server_streaming,
                }
            })
            .collect();
        self.path.pop();

        let service = Service {
            name: to_upper_camel(&name),
            proto_name: name,
            package: self.package.clone(),
            comments,
            methods,
            options: service.options.unwrap_or_default(),
        };

        self.append_service_def(service);
    }

    fn append_service_def(&mut self, service: Service) {
        // Generate a trait for the service.
        self.buf.push_str("#[generate_trait]\n");
        self.buf.push_str(&format!("impl {} of {}Trait {{\n", &service.name, &service.name));

        // Generate the service methods.
        for method in service.methods {
            self.buf.push_str(&format!(
                "    fn {}(arg: {}) -> {} {{",
                method.name, method.input_type, method.output_type
            ));

            self.buf.push_str(&format!(
                r"
                arg.send();
                cheatcode::<'oracle_ask'>(array!['{}'].span());
                Sendable::<{}>::recv()
",
method.name, method.output_type));

            self.buf.push_str("    }\n");
        }

        // Close out the trait.
        self.buf.push_str("}\n");
    }

    fn push_indent(&mut self) {
        push_indent(self.buf, self.depth);
    }

    fn push_mod(&mut self, module: &str) {
        self.push_indent();
        self.buf.push_str("/// Nested message and enum types in `");
        self.buf.push_str(module);
        self.buf.push_str("`.\n");

        self.push_indent();
        self.buf.push_str("mod ");
        self.buf.push_str(&to_snake(module));
        self.buf.push_str(" {\n");

        self.type_path.push(module.into());

        self.depth += 1;
    }

    fn pop_mod(&mut self) {
        self.depth -= 1;

        self.type_path.pop();

        self.push_indent();
        self.buf.push_str("}\n");
    }

    fn resolve_type(&self, field: &FieldDescriptorProto, fq_message_name: &str) -> String {
        match field.r#type() {
            Type::Float => String::from("f32"),
            Type::Double => String::from("f64"),
            Type::Uint32 | Type::Fixed32 => String::from("u32"),
            Type::Uint64 | Type::Fixed64 => String::from("u64"),
            Type::Int32 | Type::Sfixed32 | Type::Sint32 | Type::Enum => String::from("i32"),
            Type::Int64 | Type::Sfixed64 | Type::Sint64 => String::from("i64"),
            Type::Bool => String::from("bool"),
            Type::String => String::from("ByteArray"),
            Type::Bytes => String::from("ByteArray"),
            Type::Group | Type::Message => self.resolve_ident(field.type_name()),
        }
    }

    fn resolve_ident(&self, pb_ident: &str) -> String {
        // protoc should always give fully qualified identifiers.
        assert_eq!(".", &pb_ident[..1]);

        if let Some(proto_ident) = self.extern_paths.resolve_ident(pb_ident) {
            return proto_ident;
        }

        let mut local_path = self
            .package
            .split('.')
            .chain(self.type_path.iter().map(String::as_str))
            .peekable();

        // If no package is specified the start of the package name will be '.'
        // and split will return an empty string ("") which breaks resolution
        // The fix to this is to ignore the first item if it is empty.
        if local_path.peek().map_or(false, |s| s.is_empty()) {
            local_path.next();
        }

        let mut ident_path = pb_ident[1..].split('.');
        let ident_type = ident_path.next_back().unwrap();
        let mut ident_path = ident_path.peekable();

        // Skip path elements in common.
        while local_path.peek().is_some() && local_path.peek() == ident_path.peek() {
            local_path.next();
            ident_path.next();
        }

        local_path
            .map(|_| "super".to_string())
            .chain(ident_path.map(to_snake))
            .chain(iter::once(to_upper_camel(ident_type)))
            .join("::")
    }

    fn field_type_tag(&self, field: &FieldDescriptorProto) -> Cow<'static, str> {
        match field.r#type() {
            Type::Float => Cow::Borrowed("float"),
            Type::Double => Cow::Borrowed("double"),
            Type::Int32 => Cow::Borrowed("int32"),
            Type::Int64 => Cow::Borrowed("int64"),
            Type::Uint32 => Cow::Borrowed("uint32"),
            Type::Uint64 => Cow::Borrowed("uint64"),
            Type::Sint32 => Cow::Borrowed("sint32"),
            Type::Sint64 => Cow::Borrowed("sint64"),
            Type::Fixed32 => Cow::Borrowed("fixed32"),
            Type::Fixed64 => Cow::Borrowed("fixed64"),
            Type::Sfixed32 => Cow::Borrowed("sfixed32"),
            Type::Sfixed64 => Cow::Borrowed("sfixed64"),
            Type::Bool => Cow::Borrowed("bool"),
            Type::String => Cow::Borrowed("string"),
            Type::Bytes => Cow::Borrowed("bytes"),
            Type::Group => Cow::Borrowed("group"),
            Type::Message => Cow::Borrowed("message"),
            Type::Enum => Cow::Owned(format!(
                "enumeration={:?}",
                self.resolve_ident(field.type_name())
            )),
        }
    }

    fn map_value_type_tag(&self, field: &FieldDescriptorProto) -> Cow<'static, str> {
        match field.r#type() {
            Type::Enum => Cow::Owned(format!(
                "enumeration({})",
                self.resolve_ident(field.type_name())
            )),
            _ => self.field_type_tag(field),
        }
    }

    fn optional(&self, field: &FieldDescriptorProto) -> bool {
        if field.proto3_optional.unwrap_or(false) {
            return true;
        }

        if field.label() != Label::Optional {
            return false;
        }

        match field.r#type() {
            Type::Message => true,
            _ => self.syntax == Syntax::Proto2,
        }
    }

    /// Returns `true` if the field options includes the `deprecated` option.
    fn deprecated(&self, field: &FieldDescriptorProto) -> bool {
        field
            .options
            .as_ref()
            .map_or(false, FieldOptions::deprecated)
    }

    fn append_footer(&mut self) {
    }

    fn append_header(&mut self) {
        self.buf.push_str(
r"use starknet::testing::cheatcode;

trait Sendable<T> {
    fn send(self: @T);
    fn recv() -> T;
}

// TODO: missing implementation for builtin types
impl Sendableu64 of Sendable<u64> {
    fn send(self: @u64) {
        let val: felt252 = (*self).into();
        cheatcode::<'oracle_value_push'>(array!['u64', val].span());
    }
    fn recv() -> u64 {
        let mut bytes = cheatcode::<'oracle_value_pop'>(array!['u64'].span()); // could enforce type here!
        Serde::<u64>::deserialize(ref bytes).unwrap()
    }
}

impl Sendableu32 of Sendable<u32> {
    fn send(self: @u32) {
        let val: felt252 = (*self).into();
        cheatcode::<'oracle_value_push'>(array!['u32', val].span());
    }
    fn recv() -> u32 {
        let mut bytes = cheatcode::<'oracle_value_pop'>(array!['u32'].span()); // could enforce type here!
        Serde::<u32>::deserialize(ref bytes).unwrap()
    }
}

impl Sendablei32 of Sendable<i32> {
    fn send(self: @i32) {
        let val: felt252 = (*self).into();
        cheatcode::<'oracle_value_push'>(array!['i32', val].span());
    }
    fn recv() -> i32 {
        let mut bytes = cheatcode::<'oracle_value_pop'>(array!['i32'].span()); // could enforce type here!
        Serde::<i32>::deserialize(ref bytes).unwrap()
    }
}

impl optionimpl<T, +Sendable<T>> of Sendable<Option<T>> {
    fn send(self: @Option<T>) {
        cheatcode::<'oracle_path_push'>(array!['struct'].span());

        match self {
            Option::Some(v) => {
                cheatcode::<'oracle_key_push'>(array!['presence'].span());
                Sendable::<u64>::send(@1); // present
                cheatcode::<'oracle_key_pop'>(array!['presence'].span());

                cheatcode::<'oracle_key_push'>(array!['value'].span());
                Sendable::<T>::send(v); // present
                cheatcode::<'oracle_key_pop'>(array!['value'].span());
            },
            Option::None => {
                cheatcode::<'oracle_key_push'>(array!['presence'].span());
                Sendable::<u64>::send(@0); // not present
                cheatcode::<'oracle_key_pop'>(array!['presence'].span());
            }
        }

        cheatcode::<'oracle_path_pop'>(array!['struct'].span());
    }
    fn recv() -> Option<T> {
        Option::None
    }
}

impl ArraySendable<T, +Sendable<T>> of Sendable<Array<T>> {
    fn send(self: @Array<T>) {
        cheatcode::<'oracle_path_push'>(array!['array'].span());
        let mut i: usize = 0;
        loop {
            if i >= self.len() {
                break;
            }
            self.at(i).send();
        };
        cheatcode::<'oracle_path_pop'>(array!['array'].span());
    }

    fn recv() -> Array<T> {
        array![]
    }
}

impl ByteArraySendable of Sendable<ByteArray> {
    fn send(self: @ByteArray) {
    }

    fn recv() -> ByteArray {
        Default::default()
    }
}

");
    }
}

/// Returns `true` if the repeated field type can be packed.
fn can_pack(field: &FieldDescriptorProto) -> bool {
    matches!(
        field.r#type(),
        Type::Float
            | Type::Double
            | Type::Int32
            | Type::Int64
            | Type::Uint32
            | Type::Uint64
            | Type::Sint32
            | Type::Sint64
            | Type::Fixed32
            | Type::Fixed64
            | Type::Sfixed32
            | Type::Sfixed64
            | Type::Bool
            | Type::Enum
    )
}

/// Based on [`google::protobuf::UnescapeCEscapeString`][1]
/// [1]: https://github.com/google/protobuf/blob/3.3.x/src/google/protobuf/stubs/strutil.cc#L312-L322
fn unescape_c_escape_string(s: &str) -> Vec<u8> {
    let src = s.as_bytes();
    let len = src.len();
    let mut dst = Vec::new();

    let mut p = 0;

    while p < len {
        if src[p] != b'\\' {
            dst.push(src[p]);
            p += 1;
        } else {
            p += 1;
            if p == len {
                panic!(
                    "invalid c-escaped default binary value ({}): ends with '\'",
                    s
                )
            }
            match src[p] {
                b'a' => {
                    dst.push(0x07);
                    p += 1;
                }
                b'b' => {
                    dst.push(0x08);
                    p += 1;
                }
                b'f' => {
                    dst.push(0x0C);
                    p += 1;
                }
                b'n' => {
                    dst.push(0x0A);
                    p += 1;
                }
                b'r' => {
                    dst.push(0x0D);
                    p += 1;
                }
                b't' => {
                    dst.push(0x09);
                    p += 1;
                }
                b'v' => {
                    dst.push(0x0B);
                    p += 1;
                }
                b'\\' => {
                    dst.push(0x5C);
                    p += 1;
                }
                b'?' => {
                    dst.push(0x3F);
                    p += 1;
                }
                b'\'' => {
                    dst.push(0x27);
                    p += 1;
                }
                b'"' => {
                    dst.push(0x22);
                    p += 1;
                }
                b'0'..=b'7' => {
                    debug!("another octal: {}, offset: {}", s, &s[p..]);
                    let mut octal = 0;
                    for _ in 0..3 {
                        if p < len && src[p] >= b'0' && src[p] <= b'7' {
                            debug!("\toctal: {}", octal);
                            octal = octal * 8 + (src[p] - b'0');
                            p += 1;
                        } else {
                            break;
                        }
                    }
                    dst.push(octal);
                }
                b'x' | b'X' => {
                    if p + 3 > len {
                        panic!(
                            "invalid c-escaped default binary value ({}): incomplete hex value",
                            s
                        )
                    }
                    match u8::from_str_radix(&s[p + 1..p + 3], 16) {
                        Ok(b) => dst.push(b),
                        _ => panic!(
                            "invalid c-escaped default binary value ({}): invalid hex value",
                            &s[p..p + 2]
                        ),
                    }
                    p += 3;
                }
                _ => panic!(
                    "invalid c-escaped default binary value ({}): invalid escape",
                    s
                ),
            }
        }
    }
    dst
}

/// Strip an enum's type name from the prefix of an enum value.
///
/// This function assumes that both have been formatted to Rust's
/// upper camel case naming conventions.
///
/// It also tries to handle cases where the stripped name would be
/// invalid - for example, if it were to begin with a number.
fn strip_enum_prefix(prefix: &str, name: &str) -> String {
    let stripped = name.strip_prefix(prefix).unwrap_or(name);

    // If the next character after the stripped prefix is not
    // uppercase, then it means that we didn't have a true prefix -
    // for example, "Foo" should not be stripped from "Foobar".
    if stripped
        .chars()
        .next()
        .map(char::is_uppercase)
        .unwrap_or(false)
    {
        stripped.to_owned()
    } else {
        name.to_owned()
    }
}

struct EnumVariantMapping<'a> {
    path_idx: usize,
    proto_name: &'a str,
    proto_number: i32,
    generated_variant_name: String,
}

fn build_enum_value_mappings<'a>(
    generated_enum_name: &str,
    do_strip_enum_prefix: bool,
    enum_values: &'a [EnumValueDescriptorProto],
) -> Vec<EnumVariantMapping<'a>> {
    let mut numbers = HashSet::new();
    let mut generated_names = HashMap::new();
    let mut mappings = Vec::new();

    for (idx, value) in enum_values.iter().enumerate() {
        // Skip duplicate enum values. Protobuf allows this when the
        // 'allow_alias' option is set.
        if !numbers.insert(value.number()) {
            continue;
        }

        let mut generated_variant_name = to_upper_camel(value.name());
        if do_strip_enum_prefix {
            generated_variant_name =
                strip_enum_prefix(generated_enum_name, &generated_variant_name);
        }

        if let Some(old_v) = generated_names.insert(generated_variant_name.to_owned(), value.name())
        {
            panic!("Generated enum variant names overlap: `{}` variant name to be used both by `{}` and `{}` ProtoBuf enum values",
                generated_variant_name, old_v, value.name());
        }

        mappings.push(EnumVariantMapping {
            path_idx: idx,
            proto_name: value.name(),
            proto_number: value.number(),
            generated_variant_name,
        })
    }
    mappings
}

impl MapType {
    /// The `prost-derive` annotation type corresponding to the map type.
    pub(crate) fn annotation(&self) -> &'static str {
        match self {
            MapType::HashMap => "map",
            MapType::BTreeMap => "btree_map",
        }
    }

    /// The fully-qualified Rust type corresponding to the map type.
    pub(crate) fn rust_type(&self) -> &'static str {
        match self {
            MapType::HashMap => "::std::collections::HashMap",
            MapType::BTreeMap => "::prost::alloc::collections::BTreeMap",
        }
    }
}

impl BytesType {
    /// The `prost-derive` annotation type corresponding to the bytes type.
    pub(crate) fn annotation(&self) -> &'static str {
        match self {
            BytesType::Vec => "vec",
            BytesType::Bytes => "bytes",
        }
    }

    /// The fully-qualified Rust type corresponding to the bytes type.
    pub(crate) fn rust_type(&self) -> &'static str {
        match self {
            BytesType::Vec => "::prost::alloc::vec::Vec<u8>",
            BytesType::Bytes => "::prost::bytes::Bytes",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_c_escape_string() {
        assert_eq!(
            &b"hello world"[..],
            &unescape_c_escape_string("hello world")[..]
        );

        assert_eq!(&b"\0"[..], &unescape_c_escape_string(r#"\0"#)[..]);

        assert_eq!(
            &[0o012, 0o156],
            &unescape_c_escape_string(r#"\012\156"#)[..]
        );
        assert_eq!(&[0x01, 0x02], &unescape_c_escape_string(r#"\x01\x02"#)[..]);

        assert_eq!(
            &b"\0\x01\x07\x08\x0C\n\r\t\x0B\\\'\"\xFE"[..],
            &unescape_c_escape_string(r#"\0\001\a\b\f\n\r\t\v\\\'\"\xfe"#)[..]
        );
    }

    #[test]
    #[should_panic(expected = "incomplete hex value")]
    fn test_unescape_c_escape_string_incomplete_hex_value() {
        unescape_c_escape_string(r#"\x1"#);
    }

    #[test]
    fn test_strip_enum_prefix() {
        assert_eq!(strip_enum_prefix("Foo", "FooBar"), "Bar");
        assert_eq!(strip_enum_prefix("Foo", "Foobar"), "Foobar");
        assert_eq!(strip_enum_prefix("Foo", "Foo"), "Foo");
        assert_eq!(strip_enum_prefix("Foo", "Bar"), "Bar");
        assert_eq!(strip_enum_prefix("Foo", "Foo1"), "Foo1");
    }
}