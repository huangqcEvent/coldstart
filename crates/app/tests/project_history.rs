use std::{fs, path::PathBuf};

use coldstart_app::{ProjectHistoryStore, ProjectService};
use tempfile::TempDir;

fn create_project(root: &std::path::Path, name: &str) -> PathBuf {
    let project_home = root.join(name);
    ProjectService::new()
        .create_project(name, project_home.clone())
        .expect("project fixture must initialize");
    project_home
}

#[test]
fn empty_history_is_safe_on_first_launch() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let store = ProjectHistoryStore::new(temp.path().join("app-data"));

    let history = store.read().expect("empty history must load");

    assert!(history.last_project_path.is_none());
    assert!(history.projects.is_empty());
}

#[test]
fn remembering_a_project_persists_the_last_opened_path() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let projects = temp.path().join("projects");
    let project_home = create_project(&projects, "alpha");
    let store = ProjectHistoryStore::new(temp.path().join("app-data"));

    let history = store
        .remember(&project_home)
        .expect("valid project must be remembered");
    let reopened = ProjectHistoryStore::new(temp.path().join("app-data"))
        .read()
        .expect("persisted history must load");

    assert_eq!(history.projects.len(), 1);
    assert_eq!(history.projects[0].name, "alpha");
    assert!(history.projects[0].available);
    assert_eq!(
        history.last_project_path,
        Some(history.projects[0].root_path.clone())
    );
    assert_eq!(reopened, history);
}

#[test]
fn remembering_the_same_path_updates_one_entry_and_orders_newest_first() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let projects = temp.path().join("projects");
    let alpha = create_project(&projects, "alpha");
    let beta = create_project(&projects, "beta");
    let store = ProjectHistoryStore::new(temp.path().join("app-data"));

    store.remember(&alpha).expect("alpha must be remembered");
    store.remember(&beta).expect("beta must be remembered");
    let history = store.remember(&alpha).expect("alpha must move to front");

    assert_eq!(history.projects.len(), 2);
    assert_eq!(history.projects[0].name, "alpha");
    assert_eq!(history.projects[1].name, "beta");
    assert_eq!(
        history.last_project_path,
        Some(history.projects[0].root_path.clone())
    );
}

#[test]
fn missing_project_directories_stay_visible_but_marked_unavailable() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let project_home = create_project(temp.path().join("projects").as_path(), "alpha");
    let store = ProjectHistoryStore::new(temp.path().join("app-data"));
    store
        .remember(&project_home)
        .expect("project must be remembered");
    fs::rename(&project_home, temp.path().join("moved-alpha")).expect("project fixture must move");

    let history = store.read().expect("history must load");

    assert_eq!(history.projects.len(), 1);
    assert!(!history.projects[0].available);
    assert!(history.last_project_path.is_some());
}

#[test]
fn removing_and_clearing_history_never_deletes_project_files() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let project_home = create_project(temp.path().join("projects").as_path(), "alpha");
    let store = ProjectHistoryStore::new(temp.path().join("app-data"));
    store
        .remember(&project_home)
        .expect("project must be remembered");

    let removed = store.forget(&project_home).expect("entry must be removed");
    assert!(removed.projects.is_empty());
    assert!(removed.last_project_path.is_none());
    assert!(project_home.join("coldstart.project.json").is_file());

    store
        .remember(&project_home)
        .expect("project must be remembered");
    let cleared = store.clear().expect("history must be cleared");
    assert!(cleared.projects.is_empty());
    assert!(cleared.last_project_path.is_none());
    assert!(project_home.join("coldstart.project.json").is_file());
    assert!(project_home.join(".coldstart/state.db").is_file());
}

#[test]
fn corrupt_history_falls_back_to_an_empty_list() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let config_dir = temp.path().join("app-data");
    fs::create_dir_all(&config_dir).expect("config directory must exist");
    fs::write(config_dir.join("project-history.json"), b"{not-json")
        .expect("corrupt fixture must be written");
    let store = ProjectHistoryStore::new(config_dir);

    let history = store
        .read()
        .expect("corrupt history must not block startup");

    assert!(history.projects.is_empty());
    assert!(history.last_project_path.is_none());
}

#[test]
fn history_keeps_at_most_twelve_projects() {
    let temp = TempDir::new().expect("temporary directory must exist");
    let projects = temp.path().join("projects");
    let store = ProjectHistoryStore::new(temp.path().join("app-data"));

    for index in 0..13 {
        let project_home = create_project(&projects, &format!("project-{index:02}"));
        store
            .remember(&project_home)
            .expect("project must be remembered");
    }

    let history = store.read().expect("history must load");

    assert_eq!(history.projects.len(), 12);
    assert_eq!(history.projects[0].name, "project-12");
    assert_eq!(history.projects[11].name, "project-01");
}
