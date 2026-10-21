// Comandos da tela Python (`python_*`). Sempre registrados no
// `generate_handler!`; sem a feature `python-scripts` os de ação respondem
// `NOT_AVAILABLE` e os de leitura devolvem vazio, para a tela não quebrar.

#![allow(unused_variables)]

use std::collections::HashMap;

use super::types::*;

pub const NOT_AVAILABLE: &str = "Python automation is not available in this build";

#[cfg(feature = "python-scripts")]
const UNAVAILABLE: &str = "not implemented yet";
#[cfg(not(feature = "python-scripts"))]
const UNAVAILABLE: &str = NOT_AVAILABLE;

macro_rules! unavailable {
    () => {
        Err(UNAVAILABLE.to_string())
    };
}

#[tauri::command]
pub async fn python_runtime_status(app: tauri::AppHandle) -> Result<PythonRuntimeStatus, String> {
    Ok(PythonRuntimeStatus::default())
}

#[tauri::command]
pub async fn python_install_runtime(app: tauri::AppHandle) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_reinstall_runtime(app: tauri::AppHandle) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_workspace_info(app: tauri::AppHandle) -> Result<PythonWorkspaceInfo, String> {
    Ok(PythonWorkspaceInfo::default())
}

#[tauri::command]
pub fn python_list_scripts() -> Result<Vec<PythonScriptFile>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_read_script(name: String) -> Result<String, String> {
    unavailable!()
}

#[tauri::command]
pub fn python_write_script(name: String, source: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_create_script(name: String, template: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_delete_script(name: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_list_default_blocks() -> Result<Vec<DefaultBlockInfo>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_read_default_block(name: String) -> Result<String, String> {
    unavailable!()
}

#[tauri::command]
pub fn python_write_default_block(
    app: tauri::AppHandle,
    name: String,
    source: String,
) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_restore_default_block(name: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_open_workspace_folder() -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_open_in_editor(name: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_get_run_configs() -> Result<HashMap<String, PythonRunConfig>, String> {
    Ok(HashMap::new())
}

#[tauri::command]
pub fn python_save_run_config(config: PythonRunConfig) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_start_run(app: tauri::AppHandle, config: PythonRunConfig) -> Result<String, String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_stop_run(run_id: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_stop_all_runs() -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_list_runs() -> Result<Vec<PythonRunInfo>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_get_dashboard() -> Result<Vec<DashboardSection>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_clear_dashboard(script: String) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_inspector_capture(user_id: i64) -> Result<InspectorFrame, String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_save_asset(user_id: i64, rect: FracRect, name: String) -> Result<String, String> {
    unavailable!()
}

#[tauri::command]
pub async fn python_set_developer_mode(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    unavailable!()
}

#[tauri::command]
pub fn python_get_logs() -> Result<Vec<PythonLogLine>, String> {
    Ok(Vec::new())
}
