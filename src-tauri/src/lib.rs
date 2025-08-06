// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod odisc;
use once_cell::sync::OnceCell;
use std::fs;
use tauri::AppHandle;
use tauri::Manager;

static APP_HANDLE: OnceCell<AppHandle> = OnceCell::new();

pub fn set_app_handle(handle: AppHandle) {
    APP_HANDLE.set(handle).ok();
}

pub fn get_app_handle() -> Option<&'static AppHandle> {
    APP_HANDLE.get()
}

#[tauri::command]
async fn run_backend(app_handle: tauri::AppHandle) {
    odisc::run_backend_logic(app_handle).await;
}

#[tauri::command]
fn read_csv_file(app_handle: tauri::AppHandle) -> Result<String, String> {
    let documents_path = app_handle
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to get documents directory: {e}"))?;

    let csv_path = documents_path.join("odisc").join("mappings.csv");

    fs::read_to_string(&csv_path).map_err(|e| format!("Failed to read CSV file: {e}"))
}

#[tauri::command]
fn reload_mappings(app_handle: tauri::AppHandle) -> Result<(), String> {
    let documents_path = app_handle
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to get documents directory: {e}"))?;
    let csv_path = documents_path.join("odisc").join("mappings.csv");

    match odisc::main::load_and_log_mappings(csv_path) {
        Ok(_) => {
            println!("Reloaded mappings");
            Ok(())
        }
        Err(e) => Err(format!("Failed to reload mappings: {e}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            run_backend, 
            read_csv_file,
            reload_mappings
        ])
        .setup(|app| {
            set_app_handle(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
