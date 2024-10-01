use cainome_cairo_serde::ByteArray;
use cairo_vm::Felt252;
use serde_json::Value;
use std::str::FromStr;

use crate::{
    schema::{Schema, SchemaType},
    utils::is_valid_number,
    FuncArg, FuncArgs,
};

pub fn process_json_args(json_str: &str, schema: &Schema) -> Result<FuncArgs, String> {
    let json: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    if json.as_object().map_or(false, |obj| obj.is_empty()) {
        // Return default (empty) FuncArgs if JSON is empty
        return Ok(FuncArgs::default());
    }

    let parsed = parse_schema(&json, &schema.cairo_input, schema)?;

    Ok(FuncArgs(vec![FuncArg::Array(parsed)]))
}

fn parse_schema(value: &Value, schema_name: &str, schema: &Schema) -> Result<Vec<Felt252>, String> {
    let schema_def = schema
        .schemas
        .get(schema_name)
        .ok_or_else(|| format!("Schema {} not found in schema", schema_name))?;

    let mut args = Vec::new();

    // Iterate over the fields in the order in which they are defined.
    // This is important because the order of fields in the structure affects how they are transmitted in the VM.
    for field in &schema_def.fields {
        let field_value = value
            .get(&field.name)
            .ok_or_else(|| format!("Missing field: {} from schema {} in {}", field.name, schema_name, value))?;

        let parsed = parse_value(field_value, &field.ty, schema)?;
        args.extend(parsed);
    }

    Ok(args)
}

fn parse_value(value: &Value, ty: &SchemaType, schema: &Schema) -> Result<Vec<Felt252>, String> {
    match ty {
        SchemaType::Primitive { name } => match name.as_str() {
            "u64" | "u32" | "u16" | "u8" => {
                let num = value
                    .as_u64()
                    .ok_or_else(|| format!("Expected unsigned integer for {}", name))?;
                Ok(vec![Felt252::from(num)])
            }
            "i64" | "i32" | "i16" | "i8" => {
                let num = value
                    .as_i64()
                    .ok_or_else(|| format!("Expected signed integer for {}", name))?;
                Ok(vec![Felt252::from(num)])
            }
            "F64" => {
                let num = value
                    .as_f64()
                    .ok_or_else(|| format!("Expected float for {}", name))?;
                Ok(vec![Felt252::from((num * 2.0_f64.powi(32)) as i64)])
            }
            "felt252" => {
                let string = value
                    .as_str()
                    .ok_or_else(|| "Expected a string".to_string())?;

                // Check if the string is a valid number
                if is_valid_number(string) || string.starts_with("0x") {
                    Ok(vec![Felt252::from_str(string).map_err(|e| e.to_string())?])
                } else {
                    Ok(vec![Felt252::from_str(
                        &("0x".to_string() + &hex::encode(string)),
                    )
                    .map_err(|e| e.to_string())?])
                }
            }
            "ByteArray" => {
                let string = value
                    .as_str()
                    .ok_or_else(|| "Expected string for ByteArray".to_string())?;
                parse_byte_array(string)
            }
            "bool" => {
                let bool_value = value
                    .as_bool()
                    .ok_or_else(|| "Expected boolean value".to_string())?;
                Ok(vec![Felt252::from(bool_value as u64)])
            }
            _ => Err(format!("Unknown primitive type: {}", name)),
        },
        SchemaType::Array { item_type } | SchemaType::Span { item_type } => {
            let array = value
                .as_array()
                .ok_or_else(|| "Expected array".to_string())?;
            let mut result = Vec::new();
            result.push(Felt252::from(array.len()));
            for item in array {
                let parsed = parse_value(item, item_type, schema)?;
                result.extend(parsed);
            }
            Ok(result)
        }
        SchemaType::Struct { name } => parse_schema(value, name, schema).map(|func_args| func_args),
    }
}

fn parse_byte_array(string: &str) -> Result<Vec<Felt252>, String> {
    let byte_array =
        ByteArray::from_string(string).map_err(|e| format!("Error parsing ByteArray: {}", e))?;

    let mut result = Vec::new();
    let mut data = byte_array.data.iter().map(|b| b.felt()).collect::<Vec<_>>();
    result.push(Felt252::from(data.len()));
    result.append(&mut data);
    result.push(byte_array.pending_word);
    result.push(Felt252::from(byte_array.pending_word_len as u64));

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::schema::parse_schema_file;

    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_file_with_content(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_unsigned() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": 42});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0], FuncArg::Array(vec![Felt252::from(42)]));
    }

    #[test]
    fn test_signed() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: i32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": -42});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0], FuncArg::Array(vec![Felt252::from(-42)]));
    }

    #[test]
    fn test_f64() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: F64
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": 0.5});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![Felt252::from_hex("0x80000000").unwrap()])
        );
    }

    #[test]
    fn test_felt252() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: felt252
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        // Case 1: string is a valid number
        let json = json!({"request": "42"});
        let result = process_json_args(&json.to_string(), &input_schema).unwrap();
        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0], FuncArg::Array(vec![Felt252::from(42)]));

        // Case 2: string is a hex
        let json = json!({"request": "0x1234"});
        let result = process_json_args(&json.to_string(), &input_schema).unwrap();
        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![Felt252::from_hex("0x1234").unwrap()])
        );

        // Case 2: string is a short string
        let json = json!({"request": "hello"});
        let result = process_json_args(&json.to_string(), &input_schema).unwrap();
        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![Felt252::from_hex("0x68656c6c6f").unwrap()])
        );
    }

    #[test]
    fn test_byte_array() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: ByteArray
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": "ZK is a way of building trust in the world. The age of integrity is upon us."});
        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![
                Felt252::from_hex("0x2").unwrap(),
                Felt252::from_hex(
                    "0x5a4b206973206120776179206f66206275696c64696e672074727573742069"
                )
                .unwrap(),
                Felt252::from_hex(
                    "0x6e2074686520776f726c642e2054686520616765206f6620696e7465677269"
                )
                .unwrap(),
                Felt252::from_hex("0x74792069732075706f6e2075732e").unwrap(),
                Felt252::from_hex("0xe").unwrap(),
            ])
        );
    }

    #[test]
    fn test_bool() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: bool
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": true});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0], FuncArg::Array(vec![Felt252::from(1)]));
    }

    #[test]
    fn test_array() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Array
                        item_type:
                            type: Primitive
                            name: i32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": [1, 2, 3]});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![
                Felt252::from(3),
                Felt252::from(1),
                Felt252::from(2),
                Felt252::from(3)
            ])
        );
    }

    #[test]
    fn test_span() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Span
                        item_type:
                            type: Primitive
                            name: i32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": [1, 2, 3]});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![
                Felt252::from(3),
                Felt252::from(1),
                Felt252::from(2),
                Felt252::from(3)
            ])
        );
    }

    #[test]
    fn test_complex() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Struct
                        name: MyStruct
            MyStruct:
                fields:
                    - n:
                        type: Primitive
                        name: i64
                    - m:
                        type: Span
                        item_type:
                            type: Primitive
                            name: i32
                    - o:
                        type: Struct
                        name: Nest
            Nest:
                fields:
                    - y:
                        type: Primitive
                        name: u32
                    - z:
                        type: Span
                        item_type:
                            type: Primitive
                            name: i32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();

        let json = json!({"request": {"n": 42, "m": [1, 2, 3], "o": {"y": 42, "z": [1, 2, 3]}}});

        let result = process_json_args(&json.to_string(), &input_schema).unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            FuncArg::Array(vec![
                Felt252::from_hex("0x2a").unwrap(), // Value of "n" field
                Felt252::from_hex("0x3").unwrap(),  // Len of "m" array
                Felt252::from_hex("0x1").unwrap(),  // value of "m" array at index 0
                Felt252::from_hex("0x2").unwrap(),  // value of "m" array at index 1
                Felt252::from_hex("0x3").unwrap(),  // value of "m" array at index 2
                Felt252::from_hex("0x2a").unwrap(), // Value of "y" field
                Felt252::from_hex("0x3").unwrap(),  // Len of "z" array
                Felt252::from_hex("0x1").unwrap(),  // value of "z" array at index 0
                Felt252::from_hex("0x2").unwrap(),  // value of "z" array at index 1
                Felt252::from_hex("0x3").unwrap(),  // value of "z" array at index 2
            ])
        );
    }

    #[test]
    fn test_missing_field() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: u32
                    - optional:
                        type: Primitive
                        name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = json!({"request": 42});

        let result = process_json_args(&json.to_string(), &input_schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing field: optional"));
    }

    #[test]
    fn test_invalid_type() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = json!({"request": "NaN"});

        let result = process_json_args(&json.to_string(), &input_schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected unsigned integer"));
    }

    #[test]
    fn test_unknown_field() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = json!({"request": 42, "unknown": "extra"});

        let result = process_json_args(&json.to_string(), &input_schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_json() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = r#"{"request": 42,}"#;

        let result = process_json_args(json, &input_schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse JSON"));
    }

    #[test]
    fn test_invalid_byte_array() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Primitive
                        name: ByteArray
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = json!({"request": 12345});

        let result = process_json_args(&json.to_string(), &input_schema);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Expected string for ByteArray"));
    }

    #[test]
    fn test_invalid_array_type() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Array
                        item_type:
                            type: Primitive
                            name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = json!({"request": "not an array"});

        let result = process_json_args(&json.to_string(), &input_schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected array"));
    }

    #[test]
    fn test_invalid_nested_struct() {
        let input_schema = r#"
        schemas:
            Input:
                fields:
                    - request:
                        type: Struct
                        name: Nested
            Nested:
                fields:
                    - value:
                        type: Primitive
                        name: u32
        cairo_input: Input
        cairo_output: null
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_schema_file(&schema_file.path().to_path_buf()).unwrap();
        let json = json!({"request": {"value": "not a number"}});

        let result = process_json_args(&json.to_string(), &input_schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected unsigned integer"));
    }
}
