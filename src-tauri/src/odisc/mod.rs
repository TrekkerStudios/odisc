mod main;
use tauri::AppHandle;
use tauri::Emitter;

pub async fn run_backend_logic(app_handle: AppHandle) {
    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        if let Err(e) = main::backend(app_handle_clone).await {
            eprintln!("Error in backend: {}", e);
        }
    });

    app_handle
        .emit("backend-log", "✅ Backend started!")
        .unwrap();
}
