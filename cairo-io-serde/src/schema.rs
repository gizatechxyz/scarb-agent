use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum SchemaType {
    Primitive { name: String },
    Array { item_type: Box<SchemaType> },
    Span { item_type: Box<SchemaType> },
    Struct { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SchemaDef {
    pub(crate) fields: BTreeMap<String, SchemaType>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Schema {
    pub(crate) schemas: BTreeMap<String, SchemaDef>,
    pub(crate) cairo_input: String,
    pub(crate) cairo_output: String,
}

pub fn parse_schema_file(path: &PathBuf) -> Result<Schema, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    serde_yaml::from_str(&contents).map_err(|e| format!("Failed to parse YAML: {}", e))
}
