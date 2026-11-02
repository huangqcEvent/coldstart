use chrono::Utc;

use coldstart_core::{
    CredentialId, ProjectId, ReviewCategory, ReviewConfiguration, ReviewConfigurationInput,
    ReviewConfigurationSnapshot, ReviewContextView, ReviewFinding, ReviewScope, ReviewSeverity,
    ReviewSnapshot, ReviewStandardId, ReviewStandardSnapshot, ReviewValidationError,
    has_blocking_findings, validate_review_findings,
};

fn configuration_input() -> ReviewConfigurationInput {
    ReviewConfigurationInput {
        enabled: true,
        base_url: "https://review.example/v1".to_owned(),
        model: "reviewer".to_owned(),
        credential_id: None,
        blocking_severities: vec![ReviewSeverity::High, ReviewSeverity::Critical],
        consent_confirmed: true,
    }
}

fn finding() -> ReviewFinding {
    ReviewFinding {
        severity: ReviewSeverity::High,
        category: ReviewCategory::Security,
        file: "src/auth.rs".to_owned(),
        line: Some(42),
        message: "token is logged".to_owned(),
        recommendation: "remove the log".to_owned(),
        standard: Some("安全基线".to_owned()),
    }
}

#[test]
fn rejects_invalid_review_configuration_and_binds_consent_to_model_endpoint() {
    let project_id = ProjectId::new();
    let base = configuration_input();
    let first = ReviewConfiguration::from_input(project_id, base.clone()).expect("valid");
    assert_eq!(
        first.blocking_severities,
        vec![ReviewSeverity::Critical, ReviewSeverity::High]
    );
    assert!(first.consent_fingerprint().is_some());

    let mut invalid = base.clone();
    invalid.base_url = "file:///tmp/model".to_owned();
    assert!(matches!(
        ReviewConfiguration::from_input(project_id, invalid),
        Err(ReviewValidationError::InvalidBaseUrl(_))
    ));

    let mut changed = base;
    changed.model = "other-reviewer".to_owned();
    let other = ReviewConfiguration::from_input(project_id, changed).expect("valid");
    assert_ne!(first.consent_fingerprint(), other.consent_fingerprint());
}

#[test]
fn validates_findings_paths_limits_and_blocking_severity() {
    let finding = finding();
    assert!(validate_review_findings(vec![finding.clone()], &["src/auth.rs".to_owned()]).is_ok());
    assert!(has_blocking_findings(
        std::slice::from_ref(&finding),
        &[ReviewSeverity::High, ReviewSeverity::Critical]
    ));
    assert!(matches!(
        validate_review_findings(
            vec![ReviewFinding {
                file: "../secret".to_owned(),
                ..finding.clone()
            }],
            &["src/auth.rs".to_owned()]
        ),
        Err(ReviewValidationError::InvalidFindingPath(_))
    ));
    assert!(matches!(
        validate_review_findings(
            vec![ReviewFinding {
                message: String::new(),
                ..finding
            }],
            &["src/auth.rs".to_owned()]
        ),
        Err(ReviewValidationError::EmptyFindingMessage)
    ));
}

#[test]
fn snapshot_hash_is_stable_and_context_exposes_only_reference_metadata() {
    let project_id = ProjectId::new();
    let configuration = ReviewConfiguration::from_input(project_id, configuration_input())
        .expect("valid configuration");
    let configuration_snapshot = ReviewConfigurationSnapshot::from_configuration(&configuration);
    let standard = ReviewStandardSnapshot {
        standard_id: ReviewStandardId::new(),
        revision_id: coldstart_core::ReviewStandardRevisionId::new(),
        name: "安全基线".to_owned(),
        scope: ReviewScope::Repository("web".to_owned()),
        position: 0,
        content_hash: "a".repeat(64),
        character_count: 18,
    };
    let first = ReviewSnapshot::new(
        "web",
        "0123456789012345678901234567890123456789",
        configuration_snapshot.clone(),
        vec![standard.clone()],
    )
    .expect("snapshot");
    let same = ReviewSnapshot::new(
        "web",
        "0123456789012345678901234567890123456789",
        configuration_snapshot,
        vec![standard],
    )
    .expect("snapshot");
    assert_eq!(first.content_hash(), same.content_hash());

    let context = ReviewContextView::from_snapshot(&first);
    assert_eq!(context.standards.len(), 1);
    assert_eq!(
        context.standards[0].reference_id,
        format!("review-standard:{}", first.standards[0].standard_id)
    );
    assert_eq!(context.standards[0].name, "安全基线");
    let serialized = serde_json::to_string(&context).expect("serialize context");
    assert!(!serialized.contains("https://review.example"));
    assert!(!serialized.contains("credential"));
}

#[test]
fn configuration_can_reference_a_credential_without_exposing_it_in_context() {
    let project_id = ProjectId::new();
    let mut input = configuration_input();
    input.credential_id = Some(CredentialId::new());
    let configuration = ReviewConfiguration::from_input(project_id, input).expect("config");
    let snapshot = ReviewSnapshot::new(
        "server",
        "0123456789012345678901234567890123456789",
        ReviewConfigurationSnapshot::from_configuration(&configuration),
        Vec::new(),
    )
    .expect("snapshot");
    let context_json =
        serde_json::to_string(&ReviewContextView::from_snapshot(&snapshot)).expect("context json");
    assert!(!context_json.contains(&configuration.credential_id.expect("credential").to_string()));
    assert_eq!(snapshot.created_at.date_naive(), Utc::now().date_naive());
}
