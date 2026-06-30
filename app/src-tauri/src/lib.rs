use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub mod agents;
pub mod commands;
pub mod db;
pub mod events;
pub mod runner;
pub mod terminal;
pub mod worktree;

/// Estado global: conexión SQLite, procesos en curso (para cancelar) y el
/// terminal integrado (PTY) si está abierto.
pub struct AppState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub jobs: Arc<Mutex<HashMap<String, Child>>>,
    pub term: Arc<Mutex<Option<terminal::TermSession>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
            std::fs::create_dir_all(&dir).ok();
            let conn = db::open(dir.join("nexora.db").to_string_lossy().as_ref())
                .expect("no se pudo abrir la base de datos");
            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                jobs: Arc::new(Mutex::new(HashMap::new())),
                term: Arc::new(Mutex::new(None)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_agents,
            commands::open_project,
            commands::start_task,
            commands::cancel_task,
            commands::list_recent_tasks,
            commands::task_diff,
            commands::open_terminal,
            commands::system_stats,
            commands::list_dir,
            commands::skills_catalog,
            commands::install_skill,
            commands::read_text_file,
            terminal::term_open,
            terminal::term_write,
            terminal::term_resize
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
