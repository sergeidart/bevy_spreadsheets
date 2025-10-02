// Logic handlers extracted from the monolithic UI ai_control_panel implementation.
// This file should remain UI-framework agnostic (no egui usage) so it can be reused
// from multiple UI layouts.
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;
use pyo3::{prelude::*, exceptions::PyValueError, types::PyString};
use std::ffi::CString;

use crate::{
	sheets::{
		definitions::{ColumnDataType, SheetMetadata},
		events::{AiBatchResultKind, AiBatchTaskResult},
		resources::SheetRegistry,
	},
	ui::elements::editor::state::{AiModeState, EditorWindowState},
	ui::systems::SendEvent,
	SessionApiKey,
};

// Re-export small structs used by UI for clarity
#[derive(Clone, serde::Serialize)]
pub struct KeyPayload {
	#[serde(rename = "Context")] pub context: String,
	#[serde(rename = "Key")] pub key: String,
}

/// Parent key information for structured data (e.g., structure columns)
#[derive(Clone, serde::Serialize, Debug)]
pub struct ParentKeyInfo {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub context: Option<String>,
	pub key: String,
}

/// A group of rows belonging to a single parent (for structure requests)
#[derive(Clone, serde::Serialize, Debug)]
pub struct ParentGroup {
	pub parent_key: ParentKeyInfo,
	pub rows: Vec<Vec<String>>,
}

/// Shared payload structure for all AI batch requests (regular and structure)
#[derive(Clone, serde::Serialize)]
pub struct BatchPayload {
	pub ai_model_id: String,
	pub general_sheet_rule: Option<String>,
	pub column_contexts: Vec<Option<String>>,
	/// For regular requests: flat array of rows
	/// For structure requests: this will be empty, use parent_groups instead
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub rows_data: Vec<Vec<String>>,
	pub requested_grounding_with_google_search: bool,
	pub allow_row_additions: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub key_prefix_count: Option<usize>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub key_prefix_headers: Option<Vec<String>>,
	/// For structure requests: grouped parent-child data with clear boundaries
	#[serde(skip_serializing_if = "Option::is_none")]
	pub parent_groups: Option<Vec<ParentGroup>>,
	pub user_prompt: String,
}

#[derive(Clone)]
pub struct NonStructureColumnInfo {
	pub index: usize,
	pub context: Option<String>,
	pub data_type: ColumnDataType,
	pub included: bool,
}

pub fn rebuild_included_vectors(
	columns: &[NonStructureColumnInfo],
	indices: &mut Vec<usize>,
	contexts: &mut Vec<Option<String>>,
	data_types: &mut Vec<ColumnDataType>,
) {
	indices.clear(); contexts.clear(); data_types.clear();
	for c in columns { if c.included { indices.push(c.index); contexts.push(c.context.clone()); data_types.push(c.data_type); } }
}

/// Spawn a background python task for batch (selected rows) processing.
#[allow(clippy::too_many_arguments)]
pub fn spawn_batch_task(
	runtime: &TokioTasksRuntime,
	commands: &mut Commands,
	api_key: &SessionApiKey,
	payload_json: String,
	original_rows: Vec<usize>,
	included_cols: Vec<usize>,
	key_prefix_count: usize,
) {
	let api_key_for_task = api_key.0.clone();
	let included_cols_clone = included_cols.clone();
	let original_rows_clone = original_rows.clone();
	let key_prefix_count_clone = key_prefix_count;
	let commands_entity = commands.spawn_empty().id();
	runtime.spawn_background_task(move |mut ctx| async move {
		let api_key_value = match api_key_for_task { Some(k) if !k.is_empty() => k, _ => {
			let err_msg = "API Key not set".to_string();
			ctx.run_on_main_thread(move |world_ctx| {
				world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: original_rows_clone, result: Err(err_msg), raw_response: None, included_non_structure_columns: Vec::new(), key_prefix_count: key_prefix_count_clone, prompt_only: false, kind: AiBatchResultKind::Root { structure_context: None } } });
			}).await; return; } };
		let (result, raw_response) = tokio::task::spawn_blocking(move || {
			Python::with_gil(|py| -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>)> {
				let python_file_path = "script/ai_processor.py";
				let processor_code_string = std::fs::read_to_string(python_file_path)?;
				let code_c_str = CString::new(processor_code_string).map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
				let file_name_c_str = CString::new(python_file_path).map_err(|e| PyValueError::new_err(format!("File name CString error: {}", e)))?;
				let module_name_c_str = CString::new("ai_processor").map_err(|e| PyValueError::new_err(format!("Module name CString error: {}", e)))?;
				let module = PyModule::from_code(py, code_c_str.as_c_str(), file_name_c_str.as_c_str(), module_name_c_str.as_c_str())?;
				let binding = module.call_method1("execute_ai_query", (api_key_value, payload_json))?;
				let result_json_str: &str = binding.downcast::<PyString>()?.to_str()?;
				#[derive(serde::Deserialize)] struct PyResp { success: bool, data: Option<serde_json::Value>, error: Option<String>, raw_response: Option<String> }
				let resp: PyResp = serde_json::from_str(result_json_str).map_err(|e| PyValueError::new_err(format!("Parse JSON error: {}", e)))?;
				if resp.success { if let Some(serde_json::Value::Array(arr)) = resp.data { let mut out = Vec::new(); for row_v in arr { match row_v { serde_json::Value::Array(cells) => { out.push(cells.into_iter().map(|v| match v { serde_json::Value::String(s)=>s, other=>other.to_string() }).collect()); }, other => { return Ok((Err(format!("Row not an array: {}", other)), resp.raw_response)); } } } Ok((Ok(out), resp.raw_response)) } else { Ok((Err("Expected array of rows".to_string()), resp.raw_response)) } } else { Ok((Err(resp.error.unwrap_or_else(|| "Unknown batch error".to_string())), resp.raw_response)) } })
		}).await.unwrap_or_else(|e| Ok((Err(format!("Tokio panic: {}", e)), None)))
		.unwrap_or_else(|e| (Err(format!("PyO3 error: {}", e)), Some(e.to_string())));
		ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: original_rows_clone, result, raw_response, included_non_structure_columns: included_cols_clone, key_prefix_count: key_prefix_count_clone, prompt_only: false, kind: AiBatchResultKind::Root { structure_context: None } } }); }).await;
	});
}

/// Spawn a background python task for prompt-only (no selected rows) processing.
pub fn spawn_prompt_task(
	runtime: &TokioTasksRuntime,
	commands: &mut Commands,
	api_key: &SessionApiKey,
	payload_json: String,
	included_cols: Vec<usize>,
) {
	let api_key_for_task = api_key.0.clone();
	let included_cols_clone = included_cols.clone();
	let commands_entity = commands.spawn_empty().id();
	runtime.spawn_background_task(move |mut ctx| async move {
		let api_key_value = match api_key_for_task { Some(k) if !k.is_empty() => k, _ => {
			let err_msg = "API Key not set".to_string();
			ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: Vec::new(), result: Err(err_msg), raw_response: None, included_non_structure_columns: Vec::new(), key_prefix_count: 0, prompt_only: true, kind: AiBatchResultKind::Root { structure_context: None } } }); }).await; return; } };
		let (result, raw_response) = tokio::task::spawn_blocking(move || {
			Python::with_gil(|py| -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>)> {
				let python_file_path = "script/ai_processor.py";
				let processor_code_string = std::fs::read_to_string(python_file_path)?;
				let code_c_str = CString::new(processor_code_string).map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
				let file_name_c_str = CString::new(python_file_path).map_err(|e| PyValueError::new_err(format!("File name CString error: {}", e)))?;
				let module_name_c_str = CString::new("ai_processor").map_err(|e| PyValueError::new_err(format!("Module name CString error: {}", e)))?;
				let module = PyModule::from_code(py, code_c_str.as_c_str(), file_name_c_str.as_c_str(), module_name_c_str.as_c_str())?;
				let binding = module.call_method1("execute_ai_query", (api_key_value, payload_json))?;
				let result_json_str: &str = binding.downcast::<PyString>()?.to_str()?;
				#[derive(serde::Deserialize)] struct PyResp { success: bool, data: Option<serde_json::Value>, error: Option<String>, raw_response: Option<String> }
				let resp: PyResp = serde_json::from_str(result_json_str).map_err(|e| PyValueError::new_err(format!("Parse JSON error: {}", e)))?;
				if resp.success { if let Some(serde_json::Value::Array(arr)) = resp.data { let mut out = Vec::new(); for row_v in arr { match row_v { serde_json::Value::Array(cells) => { out.push(cells.into_iter().map(|v| match v { serde_json::Value::String(s)=>s, other=>other.to_string() }).collect()); }, other => { return Ok((Err(format!("Row not an array: {}", other)), resp.raw_response)); } } } Ok((Ok(out), resp.raw_response)) } else { Ok((Err("Expected array of rows".to_string()), resp.raw_response)) } } else { Ok((Err(resp.error.unwrap_or_else(|| "Unknown batch error".to_string())), resp.raw_response)) } })
		}).await.unwrap_or_else(|e| Ok((Err(format!("Tokio panic: {}", e)), None)))
		.unwrap_or_else(|e| (Err(format!("PyO3 error: {}", e)), Some(e.to_string())));
		ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: Vec::new(), result, raw_response, included_non_structure_columns: included_cols_clone, key_prefix_count: 0, prompt_only: true, kind: AiBatchResultKind::Root { structure_context: None } } }); }).await;
	});
}

/// Utility: resolve a structure override along a structure path.
pub fn resolve_structure_override(meta: &SheetMetadata, path: &[usize]) -> Option<bool> {
	if path.is_empty() { return None; }
	let column = meta.columns.get(path[0])?;
	if path.len() == 1 { return column.ai_enable_row_generation; }
	let mut field = column.structure_schema.as_ref()?.get(path[1])?;
	if path.len() == 2 { return field.ai_enable_row_generation; }
	for idx in path.iter().skip(2) { field = field.structure_schema.as_ref()?.get(*idx)?; }
	field.ai_enable_row_generation
}

/// Utility: describes the structure path for display (Column -> Field -> Subfield ...)
pub fn describe_structure_path(meta: &SheetMetadata, path: &[usize]) -> Option<String> {
	if path.is_empty() { return None; }
	let mut names = Vec::new();
	let column = meta.columns.get(path[0])?; names.push(column.header.clone());
	if path.len() == 1 { return Some(names.join(" -> ")); }
	let mut field = column.structure_schema.as_ref()?.get(path[1])?; names.push(field.header.clone());
	for idx in path.iter().skip(2) { field = field.structure_schema.as_ref()?.get(*idx)?; names.push(field.header.clone()); }
	Some(names.join(" -> "))
}

/// Derive a key (context, value) pair for prompt-only requests.
pub fn resolve_prompt_only_key(
	key_chain_contexts: &[Option<String>],
	key_chain_headers: &[String],
	ancestors_with_keys: &[(Option<String>, String, usize, usize)],
	registry: &SheetRegistry,
) -> Option<(String, String)> {
	for (idx, (anc_cat, anc_sheet, anc_row_idx, key_col_index)) in ancestors_with_keys.iter().enumerate() {
		let context = key_chain_contexts.get(idx).and_then(|c| c.clone()).or_else(|| key_chain_headers.get(idx).cloned());
		let sheet = registry.get_sheet(anc_cat, anc_sheet)?; let row = sheet.grid.get(*anc_row_idx)?;
		let mut value = row.get(*key_col_index).cloned().unwrap_or_default();
		if value.trim().is_empty() { value = key_chain_headers.get(idx).cloned().unwrap_or_default(); }
		if let Some(ctx) = context { return Some((ctx, value)); }
	}
	None
}

pub fn mark_submitting(state: &mut EditorWindowState) { state.ai_mode = AiModeState::Submitting; state.ai_raw_output_display.clear(); state.ai_output_panel_visible = true; state.ai_row_reviews.clear(); state.ai_new_row_reviews.clear(); state.ai_context_prefix_by_row.clear(); }
