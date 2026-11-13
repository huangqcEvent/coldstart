use coldstart_core::{
    BlueprintEntityId, CapabilityApiEndpoint, CapabilityApiEndpointInput, CapabilityApiMethod,
    CapabilityApiValidationError, CredentialId, MAX_CAPABILITY_API_QUERY_ITEMS, ProjectId,
};

fn capability_id(project_id: ProjectId) -> BlueprintEntityId {
    BlueprintEntityId::derive(&project_id, "capability", "阿里云 OSS")
}

fn input(project_id: ProjectId) -> CapabilityApiEndpointInput {
    CapabilityApiEndpointInput {
        capability_id: capability_id(project_id),
        environment: "development".to_owned(),
        name: "OSS 上传接口".to_owned(),
        base_url: "https://oss-api.example.com/v1/".to_owned(),
        path_prefix: "/attachments/".to_owned(),
        allowed_methods: vec![CapabilityApiMethod::Post, CapabilityApiMethod::Get],
        credential_id: None,
        auth_header: None,
        auth_scheme: None,
    }
}

fn endpoint() -> CapabilityApiEndpoint {
    let project_id = ProjectId::new();
    CapabilityApiEndpoint::new(project_id, input(project_id)).expect("endpoint must validate")
}

#[test]
fn validates_and_normalizes_endpoint_configuration() {
    let endpoint = endpoint();

    assert_eq!(endpoint.base_url, "https://oss-api.example.com/v1");
    assert_eq!(endpoint.path_prefix, "/attachments");
    assert_eq!(
        endpoint.allowed_methods,
        [CapabilityApiMethod::Get, CapabilityApiMethod::Post]
    );
}

#[test]
fn rejects_insecure_ambiguous_and_private_base_urls() {
    let project_id = ProjectId::new();
    for base_url in [
        "http://api.example.com",
        "https://user:pass@api.example.com",
        "https://api.example.com?token=secret",
        "https://api.example.com/#fragment",
        "https://localhost",
        "https://service.local",
        "https://127.0.0.1",
        "https://10.0.0.1",
    ] {
        let mut value = input(project_id);
        value.base_url = base_url.to_owned();
        assert!(
            CapabilityApiEndpoint::new(project_id, value).is_err(),
            "base URL must be rejected: {base_url}"
        );
    }
}

#[test]
fn rejects_path_traversal_and_names_outside_the_prefix() {
    let endpoint = endpoint();

    for path in [
        "attachments/1",
        "/attachments/../secrets",
        "/attachments/%2e%2e/secrets",
        "/attachments//file",
        "/attachments\\file",
        "/attachments?token=x",
    ] {
        assert!(
            endpoint
                .authorize_request(CapabilityApiMethod::Get, path)
                .is_err(),
            "path must be rejected: {path}"
        );
    }
    assert_eq!(
        endpoint.authorize_request(CapabilityApiMethod::Get, "/other/1"),
        Err(CapabilityApiValidationError::PathNotAllowed(
            "/other/1".to_owned()
        ))
    );
    assert!(
        endpoint
            .authorize_request(CapabilityApiMethod::Get, "/attachments/file")
            .is_ok()
    );
    assert!(
        endpoint
            .authorize_request(CapabilityApiMethod::Get, "/attachments")
            .is_ok()
    );
}

#[test]
fn base_url_path_is_preserved_when_building_the_request_url() {
    let endpoint = endpoint();
    let url = endpoint
        .resolve_url(
            "/attachments/file-name",
            &[
                ("version".to_owned(), "1".to_owned()),
                ("signed".to_owned(), "a&b".to_owned()),
            ],
        )
        .expect("URL must resolve");

    assert_eq!(url.path(), "/v1/attachments/file-name");
    assert_eq!(url.query(), Some("version=1&signed=a%26b"));
}

#[test]
fn path_validation_cannot_be_bypassed_when_get_is_not_allowed() {
    let project_id = ProjectId::new();
    let mut value = input(project_id);
    value.allowed_methods = vec![CapabilityApiMethod::Post];
    let endpoint = CapabilityApiEndpoint::new(project_id, value).expect("endpoint must validate");

    assert_eq!(
        endpoint.resolve_url("/outside", &[]),
        Err(CapabilityApiValidationError::PathNotAllowed(
            "/outside".to_owned()
        ))
    );
}

#[test]
fn rejects_method_and_authentication_configuration_errors() {
    let project_id = ProjectId::new();
    let cases = [
        (
            vec![],
            None,
            None,
            CapabilityApiValidationError::NoAllowedMethods,
        ),
        (
            vec![CapabilityApiMethod::Get, CapabilityApiMethod::Get],
            None,
            None,
            CapabilityApiValidationError::DuplicateAllowedMethod,
        ),
        (
            vec![CapabilityApiMethod::Get],
            Some("Authorization".to_owned()),
            None,
            CapabilityApiValidationError::CredentialRequiredForAuthHeader,
        ),
        (
            vec![CapabilityApiMethod::Get],
            Some("Host".to_owned()),
            Some(CredentialId::new()),
            CapabilityApiValidationError::ReservedAuthHeader("Host".to_owned()),
        ),
        (
            vec![CapabilityApiMethod::Get],
            None,
            Some(CredentialId::new()),
            CapabilityApiValidationError::AuthHeaderRequired,
        ),
    ];

    for (allowed_methods, auth_header, credential_id, expected) in cases {
        let mut value = input(project_id);
        value.allowed_methods = allowed_methods;
        value.auth_header = auth_header;
        value.credential_id = credential_id;
        let error = CapabilityApiEndpoint::new(project_id, value)
            .expect_err("configuration must be rejected");
        assert_eq!(error, expected);
    }
}

#[test]
fn rejects_oversized_query_payloads() {
    let endpoint = endpoint();
    let query = (0..=MAX_CAPABILITY_API_QUERY_ITEMS)
        .map(|index| (format!("key{index}"), "value".to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(
        endpoint.resolve_url("/attachments/file", &query),
        Err(CapabilityApiValidationError::TooManyQueryEntries)
    );
}
