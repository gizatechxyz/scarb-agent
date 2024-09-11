use cainome_cairo_serde::ByteArray;
use cairo_oracle_hint_processor::{FuncArg, FuncArgs};
use cairo_vm::Felt252;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_yaml;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum InputType {
    Primitive { name: String },
    Array { item_type: Box<InputType> },
    Span { item_type: Box<InputType> },
    Struct { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchemaDef {
    fields: BTreeMap<String, InputType>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InputSchema {
    schemas: BTreeMap<String, SchemaDef>,
    cairo_input: String,
}

pub fn parse_input_schema(path: &PathBuf) -> Result<InputSchema, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    serde_yaml::from_str(&contents).map_err(|e| format!("Failed to parse YAML: {}", e))
}

pub fn process_json_args(json_str: &str, schema: &InputSchema) -> Result<FuncArgs, String> {
    let json: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    parse_schema(&json, &schema.cairo_input, schema)
}

fn parse_value(
    value: &Value,
    ty: &InputType,
    schema: &InputSchema,
) -> Result<Vec<FuncArg>, String> {
    match ty {
        InputType::Primitive { name } => match name.as_str() {
            "u64" | "u32" | "u16" | "u8" => {
                let num = value
                    .as_u64()
                    .ok_or_else(|| format!("Expected unsigned integer for {}", name))?;
                Ok(vec![FuncArg::Single(Felt252::from(num))])
            }
            "i64" | "i32" | "i16" | "i8" => {
                let num = value
                    .as_i64()
                    .ok_or_else(|| format!("Expected signed integer for {}", name))?;
                Ok(vec![FuncArg::Single(Felt252::from(num))])
            }
            "F64" => {
                let num = value
                    .as_f64()
                    .ok_or_else(|| format!("Expected float for {}", name))?;
                Ok(vec![FuncArg::Single(Felt252::from(
                    (num * 2.0_f64.powi(32)) as i64,
                ))])
            }
            "felt252" => {
                let string = value
                    .as_str()
                    .ok_or_else(|| "Expected string for Felt252".to_string())?;
                Ok(vec![FuncArg::Single(
                    Felt252::from_str(string).map_err(|e| e.to_string())?,
                )])
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
                Ok(vec![FuncArg::Single(Felt252::from(bool_value as u64))])
            }
            _ => Err(format!("Unknown primitive type: {}", name)),
        },
        InputType::Array { item_type } | InputType::Span { item_type } => {
            let array = value
                .as_array()
                .ok_or_else(|| "Expected array".to_string())?;
            let mut result = Vec::new();
            for item in array {
                let parsed = parse_value(item, item_type, schema)?;
                result.extend(parsed);
            }
            Ok(vec![FuncArg::Array(
                result
                    .into_iter()
                    .flat_map(|arg| match arg {
                        FuncArg::Single(felt) => vec![felt],
                        FuncArg::Array(arr) => arr,
                    })
                    .collect(),
            )])
        }
        InputType::Struct { name } => {
            parse_schema(value, name, schema).map(|func_args| func_args.0)
        }
    }
}

fn parse_schema(
    value: &Value,
    schema_name: &str,
    schema: &InputSchema,
) -> Result<FuncArgs, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("Expected object for schema {}", schema_name))?;

    let schema_def = schema
        .schemas
        .get(schema_name)
        .ok_or_else(|| format!("Schema {} not found in schema", schema_name))?;

    let mut args = Vec::new();

    for (field_name, field_type) in &schema_def.fields {
        let value = obj
            .get(field_name)
            .ok_or_else(|| format!("Missing field: {} in schema {}", field_name, schema_name))?;

        let parsed = parse_value(value, field_type, schema)?;
        args.extend(parsed);
    }

    Ok(FuncArgs(args))
}

fn parse_byte_array(string: &str) -> Result<Vec<FuncArg>, String> {
    let byte_array =
        ByteArray::from_string(string).map_err(|e| format!("Error parsing ByteArray: {}", e))?;

    let mut result = Vec::new();
    let data = byte_array.data.iter().map(|b| b.felt()).collect::<Vec<_>>();
    result.push(FuncArg::Array(data));
    result.push(FuncArg::Single(byte_array.pending_word));
    result.push(FuncArg::Single(Felt252::from(
        byte_array.pending_word_len as u64,
    )));

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_file_with_content(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_parse_input_schema_and_process_json_args() {
        // Create a temporary input schema file
        let input_schema = r#"
        schemas:
          Input:
            fields:
              a:
                type: Primitive
                name: u32
              b:
                type: Primitive
                name: felt252
              c:
                type: Array
                item_type:
                  type: Primitive
                  name: i32
              d:
                type: Span
                item_type:
                  type: Struct
                  name: NestedSchema
              e:
                type: Primitive
                name: ByteArray
              f:
                type: Struct
                name: AnotherNestedSchema
              g:
                type: Primitive
                name: bool
              h:
                type: Primitive
                name: F64
              i:
                type: Span
                item_type:
                  type: Primitive
                  name: F64
          NestedSchema:
            fields:
              a:
                type: Primitive
                name: u32
              b:
                type: Primitive
                name: i32
              c:
                type: Primitive
                name: felt252
              d:
                type: Primitive
                name: ByteArray
          AnotherNestedSchema:
            fields:
              a:
                type: Primitive
                name: u32
              b:
                type: Primitive
                name: i64
        cairo_input: Input
        "#;

        let schema_file = create_temp_file_with_content(input_schema);
        let input_schema = parse_input_schema(&schema_file.path().to_path_buf()).unwrap();

        // Create JSON input
        let json = r#"
        {
            "a": 42,
            "b": "0x68656c6c6f",
            "c": [1, -2, 3],
            "d": [
                {
                    "a": 10,
                    "b": -20,
                    "c": "30",
                    "d": "ABCD"
                },
                {
                    "a": 40,
                    "b": -50,
                    "c": "-60",
                    "d": "ABCDEFGHIJKLMNOPQRSTUVWXYZ12345"
                }
            ],
            "e": "Hello world, how are you doing today?",
            "f": {
                "a": 1,
                "b": 2
            },
            "g": true,
            "h": 0.5,
            "i": [0.5, 0.5]
        }"#;

        let result = process_json_args(json, &input_schema).unwrap();

        // Assertions
        assert_eq!(result.0.len(), 12);
        assert_eq!(result.0[0], FuncArg::Single(Felt252::from(42)));
        assert_eq!(
            result.0[1],
            FuncArg::Single(Felt252::from_str("0x68656c6c6f").unwrap())
        );
        assert_eq!(
            result.0[2],
            FuncArg::Array(vec![
                Felt252::from(1),
                Felt252::from(-2i64),
                Felt252::from(3)
            ])
        );
        assert_eq!(
            result.0[3],
            FuncArg::Array(vec![
                Felt252::from(10),
                Felt252::from(-20i64),
                Felt252::from(30),
                Felt252::from_hex(
                    "0x0000000000000000000000000000000000000000000000000000000041424344"
                )
                .unwrap(),
                Felt252::from(4),
                Felt252::from(40),
                Felt252::from(-50i64),
                Felt252::from(-60i64),
                Felt252::from_hex(
                    "0x004142434445464748494a4b4c4d4e4f505152535455565758595a3132333435"
                )
                .unwrap(),
                Felt252::from(0),
                Felt252::from(0),
            ])
        );
        assert_eq!(
            result.0[4],
            FuncArg::Array(vec![Felt252::from_hex(
                "0x48656c6c6f20776f726c642c20686f772061726520796f7520646f696e6720"
            )
            .unwrap()])
        );
        assert_eq!(
            result.0[5],
            FuncArg::Single(Felt252::from_hex("0x746f6461793f").unwrap())
        );
        assert_eq!(
            result.0[6],
            FuncArg::Single(Felt252::from_hex("0x6").unwrap())
        );
        assert_eq!(result.0[7], FuncArg::Single(Felt252::from(1)));
        assert_eq!(result.0[8], FuncArg::Single(Felt252::from(2)));
        assert_eq!(result.0[9], FuncArg::Single(Felt252::from(1)));
        assert_eq!(
            result.0[10],
            FuncArg::Single(Felt252::from_hex("0x80000000").unwrap())
        );
        assert_eq!(
            result.0[11],
            FuncArg::Array(vec![
                Felt252::from_hex("0x80000000").unwrap(),
                Felt252::from_hex("0x80000000").unwrap(),
            ])
        );
    }
}
