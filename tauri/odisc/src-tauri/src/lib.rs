use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use tauri::{Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;


#[tauri::command]
fn run_backend(app_handle: tauri::AppHandle) {
    let app_handle_clone = app_handle.clone();

    thread::spawn(move || {
        let binary_path_result = if cfg!(debug_assertions) {
            Ok(std::path::PathBuf::from("target/debug/odisc-build"))
        } else {
            std::env::current_exe()
                .map_err(|e| tauri::Error::Io(e))
                .and_then(|mut path| {
                    path.pop();
                    path.push("odisc-build");
                    Ok(path)
                })
        };

        match binary_path_result {
            Ok(binary_path) => {
                if !binary_path.exists() {
                    app_handle_clone
                        .emit(
                            "backend-log",
                            format!("❌ ERROR: Binary does not exist at {:?}", binary_path),
                        )
                        .unwrap();
                    return;
                }
                if !binary_path.is_file() {
                    app_handle_clone
                        .emit(
                            "backend-log",
                            format!("❌ ERROR: Resolved path is not a file: {:?}", binary_path),
                        )
                        .unwrap();
                    return;
                }

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = fs::metadata(&binary_path) {
                        let mut perms = metadata.permissions();
                        if perms.mode() & 0o111 == 0 {
                            perms.set_mode(perms.mode() | 0o755);
                            if let Err(e) = fs::set_permissions(&binary_path, perms) {
                                app_handle_clone.emit("backend-log", format!("⚠️ Failed to set executable permissions: {}", e)).unwrap();
                            } else {
                                app_handle_clone.emit("backend-log", "✅ Ensured executable permissions on binary.").unwrap();
                            }
                        }
                    } else {
                         app_handle_clone.emit("backend-log", "⚠️ Could not get permissions for binary.").unwrap();
                    }
                }


                match StdCommand::new(&binary_path)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                {
                    Ok(mut child) => {
                        app_handle_clone
                            .emit("backend-log", "✅ Backend process started successfully.")
                            .unwrap();

                        if let Some(stdout) = child.stdout.take() {
                            let stdout_handle = app_handle_clone.clone();
                            thread::spawn(move || {
                                let reader = BufReader::new(stdout);
                                for line in reader.lines().map_while(Result::ok) {
                                    stdout_handle.emit("backend-log", format!("📤 {}", line)).unwrap();
                                }
                            });
                        }

                        if let Some(stderr) = child.stderr.take() {
                            let stderr_handle = app_handle_clone.clone();
                            thread::spawn(move || {
                                let reader = BufReader::new(stderr);
                                for line in reader.lines().map_while(Result::ok) {
                                    stderr_handle.emit("backend-log", format!("⚠️  {}", line)).unwrap();
                                }
                            });
                        }

                        let _ = child.wait();
                    }
                    Err(e) => {
                        app_handle_clone
                            .emit(
                                "backend-log",
                                format!("❌ CRITICAL: Failed to spawn process: {}", e),
                            )
                            .unwrap();
                    }
                }
            }
            Err(e) => {
                app_handle_clone
                    .emit(
                        "backend-log",
                        format!("❌ CRITICAL: Failed to resolve binary path: {}", e),
                    )
                    .unwrap();
            }
        }
    });
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
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![run_backend, read_csv_file])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}