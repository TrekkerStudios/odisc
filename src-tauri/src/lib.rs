// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod odisc;
use std::fs;
use tauri::Manager;

#[tauri::command]
async fn run_backend(app_handle: tauri::AppHandle) {
    odisc::run_backend_logic(app_handle).await;
}

#[tauri::command]
fn read_csv_file(app_handle: tauri::AppHandle) -> Result<String, String> {
    let documents_path = app_handle
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to get documents directory: {}", e.to_string()))?;

    let csv_path = documents_path.join("odisc").join("mappings.csv");

    fs::read_to_string(&csv_path).map_err(|e| format!("Failed to read CSV file: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![run_backend, read_csv_file])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
