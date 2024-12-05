use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Configuration {
    pub enums: BTreeMap<String, Vec<Mapping>>,
    pub messages: BTreeMap<String, Vec<Field>>,
    pub services: BTreeMap<String, Service>,
    pub servers_config: HashMap<String, ServerConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServerConfig {
    pub server_url: String,
    pub polling: Option<bool>,
    pub polling_config: Option<PollingConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PollingConfig {
    pub max_attempts: u64,     // Maximum number of polling attempts
    pub polling_interval: u64, // Time (in seconds) between polling attempts
    pub request_timeout: u64,  // Short timeout for each request
    pub overall_timeout: u64,  // Overall timeout
}

// primitive types supported by both Protocol Buffers and Cairo
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveType {
    U64,
    U32,
    I32,
    I64,
    BOOL,
    BYTEARRAY,
    FELT252,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Primitive(PrimitiveType),
    Message(String),
    Enum(String),
    Option(Box<FieldType>),
    Array(Box<FieldType>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Mapping {
    pub name: String,
    pub nb: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Service {
    pub methods: HashMap<String, MethodDeclaration>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MethodDeclaration {
    pub input: FieldType,
    pub output: FieldType,
}

impl From<String> for FieldType {
    fn from(value: String) -> Self {
        match value.as_ref() {
            "u64" => FieldType::Primitive(PrimitiveType::U64),
            "u32" => FieldType::Primitive(PrimitiveType::U32),
            "i32" => FieldType::Primitive(PrimitiveType::I32),
            "i64" => FieldType::Primitive(PrimitiveType::I64),
            "bool" => FieldType::Primitive(PrimitiveType::BOOL),
            "ByteArray" => FieldType::Primitive(PrimitiveType::BYTEARRAY),
            "felt252" => FieldType::Primitive(PrimitiveType::FELT252),
            _ => FieldType::Message(value),
        }
    }
}
