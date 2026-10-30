mod commands;
mod error;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::read_project_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ColdStart desktop");
}
