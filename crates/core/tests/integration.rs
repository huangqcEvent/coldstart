use std::path::PathBuf;

use coldstart_core::{
    CredentialEnvironment, CredentialKind, CredentialMetadata, IntegrationValidationError,
    ProjectId, Repository, RepositoryAuthMode, RepositorySyncPolicy, validate_branch_name,
    validate_capability_environment, validate_remote_url, validate_repository_key,
};

#[test]
fn repository_keys_and_remote_urls_are_validated() {
    assert!(validate_repository_key("server").is_ok());
    assert!(validate_repository_key("mini_program").is_ok());
    assert!(validate_repository_key("../server").is_err());
    assert!(validate_remote_url("https://user:password@example.com/repo.git").is_err());
    assert!(validate_remote_url("https://github.com/acme/repo.git").is_ok());
    assert!(validate_remote_url("git@github.com:acme/repo.git").is_ok());
    assert!(validate_remote_url("file:///D:/repos/demo.git").is_ok());
}

#[test]
fn repository_and_credential_requirements_are_enforced() {
    let project_id = ProjectId::new();
    let error = Repository::new(
        project_id,
        "server".to_owned(),
        "Server".to_owned(),
        PathBuf::from("relative"),
        "https://github.com/acme/server.git".to_owned(),
        "main".to_owned(),
        vec!["main".to_owned()],
        RepositoryAuthMode::None,
        None,
        RepositorySyncPolicy::FetchBeforeClaim,
    )
    .expect_err("relative repository path must fail");
    assert!(matches!(
        error,
        IntegrationValidationError::RepositoryPathNotAbsolute(_)
    ));

    let credential = CredentialMetadata::new(
        project_id,
        "GitHub".to_owned(),
        "github".to_owned(),
        CredentialKind::OAuthToken,
        CredentialEnvironment::Shared,
        "coldstart:secret:test".to_owned(),
    )
    .expect("credential metadata must be valid");
    assert_eq!(credential.provider, "github");
    assert!(!format!("{credential:?}").contains("gho_"));
}

#[test]
fn branch_and_capability_environment_validation_is_strict() {
    assert!(validate_branch_name("main").is_ok());
    assert!(validate_branch_name("codex/orders-1").is_ok());
    assert!(validate_branch_name("-bad").is_err());
    assert!(validate_branch_name("bad branch").is_err());
    assert!(validate_capability_environment("production").is_ok());
    assert!(validate_capability_environment("prod").is_err());
}
