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
            commands::load_delivery_overview,
            commands::read_project_memory,
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
            commands::read_repository_verification_commands,
            commands::save_repository_verification_commands,
            commands::save_credential,
            commands::delete_credential,
            commands::update_capability,
            commands::import_github_session,
            commands::start_github_device_flow,
            commands::poll_github_device_flow,
            commands::create_github_private_repository,
            commands::list_github_issues,
            commands::import_github_issue,
            commands::list_github_projects,
            commands::load_review_workspace,
            commands::save_review_configuration,
            commands::import_review_standard,
            commands::create_review_standard,
            commands::revise_review_standard,
            commands::set_review_standard_enabled,
            commands::reorder_review_standards,
            commands::delete_review_standard,
            commands::read_review_standard,
            commands::waive_work_review,
            commands::create_draft_pull_request,
            commands::refresh_pull_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ColdStart desktop");
}
