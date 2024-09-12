use cainome_cairo_serde::ByteArray;
use cairo_lang_sierra::{
    extensions::core::{CoreLibfunc, CoreType},
    ids::{ConcreteTypeId, UserTypeId},
    program::GenericArg,
    program_registry::ProgramRegistry,
};
use cairo_lang_utils::unordered_hash_map::UnorderedHashMap;
use cairo_vm::{
    math_utils::signed_felt, types::relocatable::MaybeRelocatable, vm::vm_core::VirtualMachine,
    Felt252,
};
use itertools::Itertools;
use num_traits::{cast::ToPrimitive, Zero};
use serde_json::Value as JsonValue;
use std::iter::Peekable;

use crate::schema::{Schema, SchemaType};

pub fn serialize_output(
    return_values: &[MaybeRelocatable],
    vm: &mut VirtualMachine,
    return_type_id: Option<&ConcreteTypeId>,
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
    schema: &Schema,
) -> String {
    let return_type_id = if let Some(id) = return_type_id {
        id
    } else {
        return "null".to_string();
    };
    let mut return_values_iter = return_values.iter().peekable();
    let json_value = serialize_output_inner(
        &mut return_values_iter,
        vm,
        return_type_id,
        sierra_program_registry,
        type_sizes,
        schema,  
        &schema.cairo_output,
    );

    serde_json::to_string(&json_value).unwrap_or_else(|_| "null".to_string())
}

fn serialize_output_inner<'a>(
    return_values_iter: &mut Peekable<impl Iterator<Item = &'a MaybeRelocatable>>,
    vm: &mut VirtualMachine,
    return_type_id: &ConcreteTypeId,
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
    schema: &Schema,
    current_schema_name: &str,
) -> JsonValue {
    match sierra_program_registry.get_type(return_type_id).unwrap() {
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Array(info) => {
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

            let mut json_array = Vec::new();
            while array_data_iter.peek().is_some() {
                json_array.push(serialize_output_inner(
                    &mut array_data_iter,
                    vm,
                    array_elem_id,
                    sierra_program_registry,
                    type_sizes,
                    schema,
                    current_schema_name
                ));
            }
            JsonValue::Array(json_array)
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Box(info) => {
            // As this represents a pointer, we need to extract it's values
            let ptr = return_values_iter
                .next()
                .expect("Missing return value")
                .get_relocatable()
                .expect("Box Pointer is not Relocatable");
            let type_size = type_sizes[&info.ty]
                .try_into()
                .expect("could not parse to usize");
            let data = vm
                .get_continuous_range(ptr, type_size)
                .expect("Failed to extract value from nullable ptr");
            let mut data_iter = data.iter().peekable();
            serialize_output_inner(
                &mut data_iter,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
                schema,
                current_schema_name
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Const(_) => {
            unimplemented!("Not supported in the current version")
        },
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Felt252(_) => {
            let val = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Value is not an integer");
            JsonValue::String(val.to_hex_string())
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::BoundedInt(_)
        // Only unsigned integer values implement Into<Bytes31>
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Bytes31(_) => {
            let val = return_values_iter
            .next()
            .expect("Missing return value")
            .get_int()
            .expect("Value is not an integer");

            JsonValue::String(val.to_hex_string())
        }
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint8(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint16(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint32(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint64(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint128(_) => {
            let val = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Value is not an integer");
            JsonValue::Number(val.to_u128().unwrap().into())
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint8(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint16(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint32(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint64(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint128(_) => {
            let val = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Value is not an integer");
            JsonValue::Number(signed_felt(val).to_i128().unwrap().into())
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::NonZero(info) => {
            serialize_output_inner(
                return_values_iter,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
                schema,
                current_schema_name

            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Nullable(info) => {
            // As this represents a pointer, we need to extract it's values
            match return_values_iter.next().expect("Missing return value") {
                MaybeRelocatable::RelocatableValue(ptr) => {
                    let type_size = type_sizes[&info.ty]
                        .try_into()
                        .expect("could not parse to usize");
                    let data = vm
                        .get_continuous_range(*ptr, type_size)
                        .expect("Failed to extract value from nullable ptr");
                    let mut data_iter = data.iter().peekable();
                    serialize_output_inner(
                        &mut data_iter,
                        vm,
                        &info.ty,
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
                    )
                }
                MaybeRelocatable::Int(felt) if felt.is_zero() => JsonValue::Null,
                _ => panic!("Invalid Nullable"),
            }
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
                        vm,
                        &info.variants[0],
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
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

            // Handle core::bool separately
            if let GenericArg::UserType(user_type) = &info.info.long_id.generic_args[0] {
                if user_type
                    .debug_name
                    .as_ref()
                    .is_some_and(|n| n == "core::bool")
                {
                    // Sanity checks
                    assert!(
                        *num_variants == 2
                            && variant_idx < 2
                            && type_sizes
                                .get(&info.variants[0])
                                .is_some_and(|size| size.is_zero())
                            && type_sizes
                                .get(&info.variants[1])
                                .is_some_and(|size| size.is_zero()),
                        "Malformed bool enum"
                    );

                    return JsonValue::Bool(variant_idx != 0);
                }
            }
            // TODO: Something similar to the bool handling could be done for unit enum variants if we could get the type info with the variant names

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
                vm,
                variant_type_id,
                sierra_program_registry,
                type_sizes,
                schema,
                current_schema_name

            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Struct(info) => {
            // Check if this struct is a Span
            if let Some(UserTypeId { debug_name: Some(name), .. }) = info.info.long_id.generic_args.get(0)
                .and_then(|arg| if let GenericArg::UserType(user_type) = arg { Some(user_type) } else { None }) {
                if name.starts_with("core::array::Span") {
                    if let Some(GenericArg::Type(array_type_id)) = info.info.long_id.generic_args.get(1) {
                        return serialize_output_inner(
                            return_values_iter,
                            vm,
                            array_type_id,
                            sierra_program_registry,
                            type_sizes,
                            schema,
                            current_schema_name
        
                        );
                    }
                }
            }

            // Check if this struct in a F64 
            if let Some(UserTypeId { debug_name: Some(name), .. }) = info.info.long_id.generic_args.get(0)
            .and_then(|arg| if let GenericArg::UserType(user_type) = arg { Some(user_type) } else { None }) {
            if name.starts_with("orion_numbers::f64::F64") {
                    let data = serialize_output_inner(
                        return_values_iter,
                        vm,
                        &info.members[0],
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
                    );

                    let fl = if let JsonValue::Number(scaled) = data {
                        scaled.as_f64().unwrap() / 2.0_f64.powi(32)
                    } else {
                        f64::NAN
                    };
                    let json_number = serde_json::Number::from_f64(fl).unwrap();
                    return JsonValue::Number(json_number);
                }
            }

            // Check if this struct is a ByteArray
            if let Some(UserTypeId { debug_name: Some(name), .. }) = info.info.long_id.generic_args.get(0)
                .and_then(|arg| if let GenericArg::UserType(user_type) = arg { Some(user_type) } else { None }) {
                if name == "core::byte_array::ByteArray" {
                    // Handle ByteArray
                    let data = serialize_output_inner(
                        return_values_iter,
                        vm,
                        &info.members[0],
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
                    );
                    let pending_word = serialize_output_inner(
                        return_values_iter,
                        vm,
                        &info.members[1],
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
                    );
                    let pending_word_len = serialize_output_inner(
                        return_values_iter,
                        vm,
                        &info.members[2],
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
                    );

                    // Reconstruct ByteArray
                    let byte_array = ByteArray {
                        data: if let JsonValue::Array(arr) = data {
                            arr.into_iter()
                                .map(|v| Felt252::from_hex(&v.as_str().unwrap()[2..]).unwrap().try_into().unwrap())
                                .collect()
                        } else {
                            vec![]
                        },
                        pending_word: Felt252::from_hex(&pending_word.as_str().unwrap()[2..]).unwrap(),
                        pending_word_len: pending_word_len.as_u64().unwrap() as usize,
                    };

                    // Convert to string and return
                    return match byte_array.to_string() {
                        Ok(s) => JsonValue::String(s),
                        Err(_) => JsonValue::Null,
                    };
                }
            }
            
            // If it's not a Span, F64, or ByteArray, proceed with normal struct serialization
            let mut json_object = serde_json::Map::new();
            
            let schema_def = schema.schemas.get(current_schema_name)
                .expect(&format!("Schema {} not found", current_schema_name));
            
            for (index, member_type_id) in info.members.iter().enumerate() {
                let field_info = schema_def.fields.iter().nth(index);
                
                if let Some((field_name, field_type)) = field_info {
                    json_object.insert(
                        field_name.clone(),
                        serialize_output_inner(
                            return_values_iter,
                            vm,
                            member_type_id,
                            sierra_program_registry,
                            type_sizes,
                            schema,
                            match field_type {
                                SchemaType::Struct { name } => name,
                                _ => current_schema_name,
                            },
                        ),
                    );
                }
            }
            JsonValue::Object(json_object)
        },
         cairo_lang_sierra::extensions::core::CoreTypeConcrete::Felt252Dict(info)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::SquashedFelt252Dict(info) => {
            let (dict_start, dict_size) = match sierra_program_registry
                .get_type(return_type_id)
                .unwrap()
            {
                cairo_lang_sierra::extensions::core::CoreTypeConcrete::Felt252Dict(_) => {
                    let dict_ptr = return_values_iter
                        .next()
                        .expect("Missing return val")
                        .get_relocatable()
                        .expect("Dict Ptr not Relocatable");
                    if !(dict_ptr.offset
                        == vm
                            .get_segment_size(dict_ptr.segment_index as usize)
                            .unwrap_or_default()
                        && dict_ptr.offset % 3 == 0)
                    {
                        panic!("Return value is not a valid Felt252Dict")
                    }
                    ((dict_ptr.segment_index, 0).into(), dict_ptr.offset)
                }
                cairo_lang_sierra::extensions::core::CoreTypeConcrete::SquashedFelt252Dict(_) => {
                    let dict_start = return_values_iter
                        .next()
                        .expect("Missing return val")
                        .get_relocatable()
                        .expect("Squashed dict_start ptr not Relocatable");
                    let dict_end = return_values_iter
                        .next()
                        .expect("Missing return val")
                        .get_relocatable()
                        .expect("Squashed dict_end ptr not Relocatable");
                    let dict_size = (dict_end - dict_start).unwrap();
                    if dict_size % 3 != 0 {
                        panic!("Return value is not a valid SquashedFelt252Dict")
                    }
                    (dict_start, dict_size)
                }
                _ => unreachable!(),
            };

            let value_type_id = &info.ty;
            let dict_mem = vm
                .get_continuous_range(dict_start, dict_size)
                .expect("Malformed dictionary memory");

            let mut json_object = serde_json::Map::new();
            for (key, _, value) in dict_mem.iter().tuples() {
                let key_string = key.to_string();
                let value_vec = vec![value.clone()];
                let mut value_iter = value_vec.iter().peekable();
                json_object.insert(
                    key_string,
                    serialize_output_inner(
                        &mut value_iter,
                        vm,
                        value_type_id,
                        sierra_program_registry,
                        type_sizes,
                        schema,
                        current_schema_name
    
                    ),
                );
            }
            JsonValue::Object(json_object)
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Snapshot(info) => {
            serialize_output_inner(
                return_values_iter,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
                schema,
                current_schema_name
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::GasBuiltin(_info) => {
            // Ignore it
            let _ = return_values_iter.next();
            JsonValue::Null
        }
        _ => panic!("Unexpected return type"),
    }
}
