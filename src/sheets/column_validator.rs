// src/sheets/definitions/column_validator.rs
use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};
use std::fmt;

use super::column_data_type::{parse_column_data_type, ColumnDataType};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ColumnValidator {
    Basic(ColumnDataType),
    Linked {
        target_sheet_name: String,
        target_column_index: usize,
    },
    // Schema B: Structure validator (schema embedded elsewhere, no indices here)
    Structure,
}

// Custom Deserialize for backward compatibility (accept legacy Structure { source_column_indices: [...] })
impl<'de> Deserialize<'de> for ColumnValidator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(s) = value.as_str() {
            return match s {
                "Structure" => Ok(ColumnValidator::Structure),
                other => parse_column_data_type(other)
                    .map(ColumnValidator::Basic)
                    .ok_or_else(|| {
                        de::Error::custom(format!("Unknown ColumnValidator string '{}'.", other))
                    }),
            };
        }
        if let Some(obj) = value.as_object() {
            if obj.len() == 1 {
                let (tag, inner) = obj.iter().next().unwrap();
                match tag.as_str() {
                    "Basic" => {
                        if inner.is_string() {
                            if let Some(dt) = inner.as_str().and_then(parse_column_data_type) {
                                return Ok(ColumnValidator::Basic(dt));
                            }
                        }
                        let dt: ColumnDataType =
                            serde_json::from_value(inner.clone()).map_err(|e| {
                                de::Error::custom(format!("Invalid Basic validator payload: {}", e))
                            })?;
                        return Ok(ColumnValidator::Basic(dt));
                    }
                    "Linked" => {
                        #[derive(Deserialize)]
                        struct LinkedHelper {
                            target_sheet_name: String,
                            target_column_index: usize,
                        }
                        let helper: LinkedHelper =
                            serde_json::from_value(inner.clone()).map_err(|e| {
                                de::Error::custom(format!(
                                    "Invalid Linked validator payload: {}",
                                    e
                                ))
                            })?;
                        return Ok(ColumnValidator::Linked {
                            target_sheet_name: helper.target_sheet_name,
                            target_column_index: helper.target_column_index,
                        });
                    }
                    "Structure" => {
                        return Ok(ColumnValidator::Structure);
                    }
                    _ => {}
                }
            }
        }
        Err(de::Error::custom(
            "Unrecognized ColumnValidator representation",
        ))
    }
}

impl fmt::Display for ColumnValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnValidator::Basic(data_type) => write!(f, "Basic({})", data_type),
            ColumnValidator::Linked {
                target_sheet_name,
                target_column_index,
            } => {
                write!(
                    f,
                    "Linked{{target_sheet_name: \"{}\", target_column_index: {}}}",
                    target_sheet_name, target_column_index
                )
            }
            ColumnValidator::Structure => write!(f, "Structure"),
        }
    }
}

pub fn parse_legacy_validator(raw: &str, fallback_type: ColumnDataType) -> Option<ColumnValidator> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(stripped) = trimmed
        .strip_prefix("Basic(")
        .and_then(|r| r.strip_suffix(')'))
    {
        if let Some(dt) = parse_column_data_type(stripped) {
            return Some(ColumnValidator::Basic(dt));
        }
        return Some(ColumnValidator::Basic(fallback_type));
    }
    if let Some(dt) = parse_column_data_type(trimmed) {
        return Some(ColumnValidator::Basic(dt));
    }
    Some(ColumnValidator::Basic(fallback_type))
}
