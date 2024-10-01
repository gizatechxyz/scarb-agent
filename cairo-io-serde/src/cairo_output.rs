use std::{collections::VecDeque, iter::Peekable};

use cairo_lang_sierra::{
    extensions::core::{CoreLibfunc, CoreType},
    ids::ConcreteTypeId,
    program::GenericArg,
    program_registry::ProgramRegistry,
};
use cairo_lang_utils::unordered_hash_map::UnorderedHashMap;
use cairo_vm::{
    math_utils::signed_felt, types::relocatable::MaybeRelocatable, vm::vm_core::VirtualMachine,
    Felt252,
};
use num_traits::cast::ToPrimitive;
use serde_json::{json, Value};

use crate::schema::{Schema, SchemaType};

pub fn process_output(output: Vec<Felt252>, schema: &Schema) -> Result<String, String> {
    let schema_name = &schema.cairo_output;
    let mut output_queue: VecDeque<Felt252> = output.into();

    let parsed = parse_schema(&mut output_queue, schema_name, schema)?;

    serde_json::to_string_pretty(&parsed).map_err(|e| format!("Failed to serialize to JSON: {}", e))
}

fn parse_schema(
    output_queue: &mut VecDeque<Felt252>,
    schema_name: &str,
    schema: &Schema,
) -> Result<Value, String> {
    let schema_def = schema
        .schemas
        .get(schema_name)
        .ok_or_else(|| format!("Schema {} not found in schema", schema_name))?;

    let mut result = json!({});

    for field in &schema_def.fields {
        let parsed = parse_value(output_queue, &field.ty, schema)?;
        result[&field.name] = parsed;
    }

    Ok(result)
}

fn parse_value(
    output_queue: &mut VecDeque<Felt252>,
    ty: &SchemaType,
    schema: &Schema,
) -> Result<Value, String> {
    match ty {
        SchemaType::Primitive { name } => match name.as_str() {
            "u64" | "u32" | "u16" | "u8" => {
                let value = output_queue.pop_front().ok_or("Unexpected end of output")?;
                Ok(json!(value.to_u64()))
            }
            "i64" | "i32" | "i16" | "i8" => {
                let value = output_queue.pop_front().ok_or("Unexpected end of output")?;
                Ok(json!(signed_felt(value).to_i64()))
            }
            "F64" => {
                let value = output_queue.pop_front().ok_or("Unexpected end of output")?;
                let float_value = (value.to_i64().unwrap() as f64) / 2f64.powi(32);
                Ok(json!(float_value))
            }
            "felt252" => {
                let value = output_queue.pop_front().ok_or("Unexpected end of output")?;
                Ok(json!(value.to_hex_string()))
            }
            "ByteArray" => {
                let length = output_queue
                    .pop_front()
                    .ok_or("Unexpected end of output")?
                    .to_usize()
                    .unwrap();
                let mut bytes = Vec::new();
                for _ in 0..length {
                    let byte = output_queue.pop_front().ok_or("Unexpected end of output")?;
                    bytes.push(byte.to_u8().unwrap());
                }
                let pending_word = output_queue.pop_front().ok_or("Unexpected end of output")?;
                let pending_word_len =
                    output_queue.pop_front().ok_or("Unexpected end of output")?;

                let mut result = String::from_utf8(bytes)
                    .map_err(|e| format!("Invalid UTF-8 sequence: {}", e))?;
                if pending_word_len.to_usize().unwrap() > 0 {
                    result.push_str(&pending_word.to_string());
                }

                Ok(json!(result))
            }
            "bool" => {
                let value = output_queue.pop_front().ok_or("Unexpected end of output")?;
                Ok(json!(value != Felt252::ZERO))
            }
            _ => Err(format!("Unknown primitive type: {}", name)),
        },
        SchemaType::Array { item_type } | SchemaType::Span { item_type } => {
            let length = output_queue
                .pop_front()
                .ok_or("Unexpected end of output")?
                .to_usize()
                .unwrap();
            let mut result = Vec::new();
            for _ in 0..length {
                let parsed = parse_value(output_queue, item_type, schema)?;
                result.push(parsed);
            }
            Ok(json!(result))
        }
        SchemaType::Struct { name } => parse_schema(output_queue, name, schema),
    }
}

pub fn serialize_output(
    return_values: &[MaybeRelocatable],
    vm: &mut VirtualMachine,
    return_type_id: Option<&ConcreteTypeId>,
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
) -> Vec<Felt252> {
    let mut output_vec = Vec::new();
    let return_type_id = if let Some(id) = return_type_id {
        id
    } else {
        return output_vec;
    };
    let mut return_values_iter = return_values.iter().peekable();
    serialize_output_inner(
        &mut return_values_iter,
        &mut output_vec,
        vm,
        return_type_id,
        sierra_program_registry,
        type_sizes,
    );

    output_vec
}

fn serialize_output_inner<'a>(
    return_values_iter: &mut Peekable<impl Iterator<Item = &'a MaybeRelocatable>>,
    output_vec: &mut Vec<Felt252>,
    vm: &mut VirtualMachine,
    return_type_id: &ConcreteTypeId,
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
) {
    match sierra_program_registry.get_type(return_type_id).unwrap() {
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Array(info) => {
            // Fetch array from memory
            let array_start = return_values_iter
                .next()
                .expect("Missing return value")
                .get_relocatable()
                .expect("Array start_ptr not Relocatable");
            let array_end = return_values_iter
                .next()
                .expect("Missing return value")
                .get_relocatable()
                .expect("Array end_ptr not Relocatable");
            let array_size = (array_end - array_start).unwrap();

            let array_data = vm.get_continuous_range(array_start, array_size).unwrap();
            let mut array_data_iter = array_data.iter().peekable();
            let array_elem_id = &info.ty;
            // Serialize array data
            while array_data_iter.peek().is_some() {
                serialize_output_inner(
                    &mut array_data_iter,
                    output_vec,
                    vm,
                    array_elem_id,
                    sierra_program_registry,
                    type_sizes,
                )
            }
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Felt252(_) => {
            let val = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Value is not an integer");
            output_vec.push(val);
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Enum(info) => {
            // First we check if it is a Panic enum, as we already handled panics when fetching return values,
            // we can ignore them and move on to the non-panic variant
            if let GenericArg::UserType(user_type) = &info.info.long_id.generic_args[0] {
                if user_type
                    .debug_name
                    .as_ref()
                    .is_some_and(|n| n.starts_with("core::panics::PanicResult"))
                {
                    return serialize_output_inner(
                        return_values_iter,
                        output_vec,
                        vm,
                        &info.variants[0],
                        sierra_program_registry,
                        type_sizes,
                    );
                }
            }
            let num_variants = &info.variants.len();
            let casm_variant_idx: usize = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Enum tag is not integer")
                .to_usize()
                .expect("Invalid enum tag");
            // Convert casm variant idx to sierra variant idx
            let variant_idx = if *num_variants > 2 {
                num_variants - 1 - (casm_variant_idx >> 1)
            } else {
                casm_variant_idx
            };
            let variant_type_id = &info.variants[variant_idx];

            // Space is always allocated for the largest enum member, padding with zeros in front for the smaller variants
            let mut max_variant_size = 0;
            for variant in &info.variants {
                let variant_size = type_sizes.get(variant).unwrap();
                max_variant_size = std::cmp::max(max_variant_size, *variant_size)
            }
            for _ in 0..max_variant_size - type_sizes.get(variant_type_id).unwrap() {
                // Remove padding
                assert_eq!(
                    return_values_iter.next(),
                    Some(&MaybeRelocatable::from(0)),
                    "Malformed enum"
                );
            }
            serialize_output_inner(
                return_values_iter,
                output_vec,
                vm,
                variant_type_id,
                sierra_program_registry,
                type_sizes,
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Struct(info) => {
            for member_type_id in &info.members {
                serialize_output_inner(
                    return_values_iter,
                    output_vec,
                    vm,
                    member_type_id,
                    sierra_program_registry,
                    type_sizes,
                )
            }
        }
        _ => panic!("Unexpected return type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse_schema_file;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_file_with_content(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_process_output_primitive_types() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - unsigned:
                        type: Primitive
                        name: u32
                    - signed:
                        type: Primitive
                        name: i32
                    - float:
                        type: Primitive
                        name: F64
                    - felt:
                        type: Primitive
                        name: felt252
                    - boolean:
                        type: Primitive
                        name: bool
        cairo_input: null
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![
            Felt252::from(42),
            Felt252::from(-42),
            Felt252::from_hex("0x80000000").unwrap(), // 0.5 in fixed-point representation
            Felt252::from_hex("0x1234").unwrap(),
            Felt252::from(1),
        ];

        let result = process_output(output, &schema).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["unsigned"], 42);
        assert_eq!(parsed["signed"], -42);
        assert_eq!(parsed["float"], 0.5);
        assert_eq!(parsed["felt"], "0x1234");
        assert_eq!(parsed["boolean"], true);
    }

    #[test]
    fn test_process_output_array_and_struct() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - array:
                        type: Array
                        item_type:
                            type: Primitive
                            name: u32
                    - nested:
                        type: Struct
                        name: Nested
            Nested:
                fields:
                    - value:
                        type: Primitive
                        name: u32
                    - inner_array:
                        type: Array
                        item_type:
                            type: Primitive
                            name: u32
        cairo_input: Input
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![
            Felt252::from(3), // Length of the first array
            Felt252::from(1),
            Felt252::from(2),
            Felt252::from(3),
            Felt252::from(42), // Nested struct's value
            Felt252::from(2),  // Length of the inner array
            Felt252::from(4),
            Felt252::from(5),
        ];

        let result = process_output(output, &schema).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["array"], json!([1, 2, 3]));
        assert_eq!(parsed["nested"]["value"], 42);
        assert_eq!(parsed["nested"]["inner_array"], json!([4, 5]));
    }

    #[test]
    fn test_process_output_byte_array() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - byte_array:
                        type: Primitive
                        name: ByteArray
        cairo_input: Input
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![
            Felt252::from(13),  // Length of the byte array
            Felt252::from(72),  // 'H'
            Felt252::from(101), // 'e'
            Felt252::from(108), // 'l'
            Felt252::from(108), // 'l'
            Felt252::from(111), // 'o'
            Felt252::from(44),  // ','
            Felt252::from(32),  // ' '
            Felt252::from(87),  // 'W'
            Felt252::from(111), // 'o'
            Felt252::from(114), // 'r'
            Felt252::from(108), // 'l'
            Felt252::from(100), // 'd'
            Felt252::from(33),  // '!'
            Felt252::from(0),   // Pending word
            Felt252::from(0),   // Pending word length
        ];

        let result = process_output(output, &schema).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["byte_array"], "Hello, World!");
    }

    #[test]
    fn test_insufficient_output_data() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - value:
                        type: Primitive
                        name: u32
        cairo_input: null
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![]; // Empty output

        let result = process_output(output, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unexpected end of output"));
    }

    #[test]
    fn test_invalid_primitive_type() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - value:
                        type: Primitive
                        name: invalid_type
        cairo_input: null
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![Felt252::from(42)];

        let result = process_output(output, &schema);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Unknown primitive type: invalid_type"));
    }

    #[test]
    fn test_invalid_array_length() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - array:
                        type: Array
                        item_type:
                            type: Primitive
                            name: u32
        cairo_input: null
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![Felt252::from(3), Felt252::from(1), Felt252::from(2)]; // Declared length 3, but only 2 elements

        let result = process_output(output, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unexpected end of output"));
    }

    #[test]
    fn test_invalid_byte_array() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - byte_array:
                        type: Primitive
                        name: ByteArray
        cairo_input: null
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![
            Felt252::from(2),   // Length
            Felt252::from(255), // Invalid UTF-8 byte
            Felt252::from(255), // Invalid UTF-8 byte
            Felt252::from(0),   // Pending word
            Felt252::from(0),   // Pending word length
        ];

        let result = process_output(output, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid UTF-8 sequence"));
    }

    #[test]
    fn test_missing_schema() {
        let schema_content = r#"
        schemas:
            Output:
                fields:
                    - value:
                        type: Struct
                        name: MissingStruct
        cairo_input: null
        cairo_output: Output
        "#;

        let schema_file = create_temp_file_with_content(schema_content);
        let schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let output = vec![Felt252::from(42)];

        let result = process_output(output, &schema);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Schema MissingStruct not found in schema"));
    }
}
