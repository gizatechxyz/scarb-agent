use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;
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
    pub(crate) fields: Vec<NamedSchemaType>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NamedSchemaType {
    pub(crate) name: String,
    pub(crate) ty: SchemaType,
}

impl<'de> Deserialize<'de> for NamedSchemaType {
    fn deserialize<D>(deserializer: D) -> Result<NamedSchemaType, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(NamedSchemaTypeVisitor)
    }
}

struct NamedSchemaTypeVisitor;

impl<'de> Visitor<'de> for NamedSchemaTypeVisitor {
    type Value = NamedSchemaType;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map with a single key-value pair")
    }

    fn visit_map<M>(self, mut map: M) -> Result<NamedSchemaType, M::Error>
    where
        M: MapAccess<'de>,
    {
        if let Some((key, value)) = map.next_entry::<String, SchemaType>()? {
            if map.next_key::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("Expected only one key per field"));
            }
            Ok(NamedSchemaType {
                name: key,
                ty: value,
            })
        } else {
            Err(de::Error::custom("Expected at least one key-value pair"))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Schema {
    pub(crate) schemas: HashMap<String, SchemaDef>,
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
