// src/sheets/definitions/column_data_type.rs
use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
pub enum ColumnDataType {
    #[default]
    String,
    Bool,
    I64,
    F64,
}

impl fmt::Display for ColumnDataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// Custom Deserialize to keep backward compatibility with removed variants
impl<'de> Deserialize<'de> for ColumnDataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        let as_str = match v {
            serde_json::Value::String(s) => s,
            other => {
                return Err(de::Error::custom(format!(
                    "ColumnDataType must be string, got {}",
                    other
                )))
            }
        };
        parse_column_data_type(&as_str)
            .ok_or_else(|| de::Error::custom(format!("Unknown ColumnDataType '{}'", as_str)))
    }
}

pub fn parse_column_data_type(s: &str) -> Option<ColumnDataType> {
    let norm = s.trim();
    // Accept exact Debug variants or lowercase
    match norm {
        // Supported canonical variants
        "String" | "string" | "OptionString" | "optionstring" | "Option<String>" => {
            Some(ColumnDataType::String)
        }
        "Bool" | "bool" | "OptionBool" | "optionbool" | "Option<Bool>" => {
            Some(ColumnDataType::Bool)
        }
        "I64" | "i64" | "Int" | "int" | "OptionI64" | "optioni64" | "Option<Int>"
        | "Option<int>" => Some(ColumnDataType::I64),
        "F64" | "f64" | "Float" | "float" | "OptionF64" | "optionf64" | "Option<Float>"
        | "Option<float>" => Some(ColumnDataType::F64),
        // Legacy integer widths map to I64
        "U8" | "u8" | "U16" | "u16" | "U32" | "u32" | "U64" | "u64" | "I8" | "i8" | "I16"
        | "i16" | "I32" | "i32" => Some(ColumnDataType::I64),
        "OptionU8" | "optionu8" | "OptionU16" | "optionu16" | "OptionU32" | "optionu32"
        | "OptionU64" | "optionu64" | "OptionI8" | "optioni8" | "OptionI16" | "optioni16"
        | "OptionI32" | "optioni32" => Some(ColumnDataType::I64),
        // Legacy float f32 maps to F64
        "F32" | "f32" => Some(ColumnDataType::F64),
        "OptionF32" | "optionf32" => Some(ColumnDataType::F64),
        _ => None,
    }
}
