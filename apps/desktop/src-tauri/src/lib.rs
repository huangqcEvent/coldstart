mod commands;
mod error;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::read_project_status,
            commands::read_requirements_workspace,
            commands::import_requirement_document,
            commands::generate_blueprint,
            commands::save_blueprint_draft,
            commands::publish_blueprint,
            commands::resolve_info_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ColdStart desktop");
}
