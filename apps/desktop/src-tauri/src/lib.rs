mod commands;
mod error;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(std::sync::Arc::new(coldstart_app::IntegrationService::new()))
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::read_project_status,
            commands::read_requirements_workspace,
            commands::import_requirement_document,
            commands::generate_blueprint,
            commands::save_blueprint_draft,
            commands::publish_blueprint,
            commands::resolve_info_request,
            commands::read_plan_workspace,
            commands::generate_plan_draft,
            commands::save_plan_draft,
            commands::preview_plan_publish,
            commands::publish_plan,
            commands::complete_human_work,
            commands::read_integration_workspace,
            commands::validate_repository,
            commands::save_repository,
            commands::save_credential,
            commands::delete_credential,
            commands::update_capability,
            commands::import_github_session,
            commands::start_github_device_flow,
            commands::poll_github_device_flow,
            commands::create_github_private_repository
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ColdStart desktop");
}
