// src/sheets/systems/ai/structure_processor/python_executor.rs
//! Python AI query execution and response parsing

use bevy::prelude::*;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use std::ffi::CString;

/// Rewrite the Python processor file to ensure it's up to date before execution
pub fn rewrite_python_processor() {
    const AI_PROCESSOR_PY: &str = include_str!("../../../../../script/ai_processor.py");
    let script_path = std::path::Path::new("script/ai_processor.py");
    if let Some(parent) = script_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(script_path, AI_PROCESSOR_PY);
}

/// Execute Python AI query and parse response
///
/// Returns (result, raw_response, updated_partitions)
pub async fn execute_python_ai_query(
    api_key: String,
    payload_json: String,
) -> (Result<Vec<Vec<String>>, String>, Option<String>, Option<Vec<usize>>) {
    tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>, Option<Vec<usize>>)> {
            let python_file_path = "script/ai_processor.py";
            let processor_code_string = std::fs::read_to_string(python_file_path)?;
            let code_c_str = CString::new(processor_code_string)
                .map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
            let file_name_c_str = CString::new(python_file_path)
                .map_err(|e| PyValueError::new_err(format!("File name CString error: {}", e)))?;
            let module_name_c_str = CString::new("ai_processor")
                .map_err(|e| PyValueError::new_err(format!("Module name CString error: {}", e)))?;

            let module = PyModule::from_code(py, code_c_str.as_c_str(), file_name_c_str.as_c_str(), module_name_c_str.as_c_str())?;
            let binding = module.call_method1("execute_ai_query", (api_key, payload_json))?;
            let result_str: &str = binding.downcast::<PyString>()?.to_str()?;

            parse_ai_response(result_str)
        })
    })
    .await
    .unwrap_or_else(|e| Ok((Err(format!("Tokio panic: {}", e)), None, None)))
    .unwrap_or_else(|e| (Err(format!("PyO3 error: {}", e)), Some(e.to_string()), None))
}

/// Parse AI response JSON
fn parse_ai_response(
    response_text: &str,
) -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>, Option<Vec<usize>>)> {
    let parsed: serde_json::Value = serde_json::from_str(response_text)
        .map_err(|e| PyValueError::new_err(format!("JSON parse error: {}", e)))?;

    let success = parsed.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    if !success {
        return Ok((
            Err(parsed.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error").to_string()),
            parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()),
            None,
        ));
    }

    let Some(data) = parsed.get("data").and_then(|v| v.as_array()) else {
        return Ok((Err("Expected data array".to_string()), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), None));
    };

    // Check if grouped (3D) or flat (2D) response
    let is_grouped = !data.is_empty()
        && data[0].is_array()
        && data[0].as_array().map_or(false, |arr| !arr.is_empty() && arr[0].is_array());

    if is_grouped {
        parse_grouped_response(data, &parsed)
    } else {
        parse_flat_response(data, &parsed)
    }
}

/// Parse grouped (3D) response
fn parse_grouped_response(
    data: &[serde_json::Value],
    parsed: &serde_json::Value,
) -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>, Option<Vec<usize>>)> {
    info!("Detected grouped response with {} groups", data.len());
    let mut out = Vec::new();
    let mut partitions = Vec::new();

    for (group_idx, group_val) in data.iter().enumerate() {
        let Some(group_array) = group_val.as_array() else {
            return Ok((Err(format!("Group {} not an array", group_idx)), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), None));
        };

        partitions.push(group_array.len());
        for row_val in group_array {
            let Some(row_array) = row_val.as_array() else {
                return Ok((Err(format!("Group {} row not an array", group_idx)), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), None));
            };

            let row: Vec<String> = row_array.iter().map(|cell| cell.as_str().unwrap_or("").to_string()).collect();
            out.push(row);
        }
    }

    info!("Flattened {} groups into {} total rows", data.len(), out.len());
    Ok((Ok(out), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), Some(partitions)))
}

/// Parse flat (2D) response
fn parse_flat_response(
    data: &[serde_json::Value],
    parsed: &serde_json::Value,
) -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>, Option<Vec<usize>>)> {
    info!("Detected flat response with {} rows", data.len());
    let mut out = Vec::new();

    for row_val in data {
        let Some(row_array) = row_val.as_array() else {
            return Ok((Err("Row not an array".to_string()), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), None));
        };

        let row: Vec<String> = row_array.iter().map(|cell| cell.as_str().unwrap_or("").to_string()).collect();
        out.push(row);
    }

    Ok((Ok(out), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), None))
}
