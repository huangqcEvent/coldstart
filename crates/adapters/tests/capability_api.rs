use coldstart_adapters::{
    CapabilityApiClient, CapabilityApiError, CapabilityApiHttpRequest, UreqCapabilityApiClient,
};
use coldstart_core::{
    CapabilityApiMethod, MAX_CAPABILITY_API_REQUEST_BODY_BYTES, MAX_CAPABILITY_API_RESPONSE_BYTES,
};
use tiny_http::{Header, Response, Server, StatusCode};

const SECRET: &str = "capability-secret-marker";

fn local_url(server: &Server) -> String {
    format!("http://{}", server.server_addr())
}

#[test]
fn injects_configured_headers_and_reads_a_successful_response() {
    let server = Server::http("127.0.0.1:0").expect("mock server must start");
    let base_url = local_url(&server);
    let handle = std::thread::spawn(move || {
        let mut request = server.recv().expect("request must arrive");
        assert_eq!(request.method().to_string(), "POST");
        assert_eq!(request.url(), "/v1/files?mode=test");
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_owned());
        assert_eq!(
            authorization.as_deref(),
            Some("Bearer capability-secret-marker")
        );
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("request body must be readable");
        assert_eq!(body, "{\"name\":\"demo\"}");
        let mut response = Response::from_string("{\"ok\":true}");
        response.add_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8")
                .expect("header must be valid"),
        );
        request.respond(response).expect("response must send");
    });

    let response = UreqCapabilityApiClient::for_testing()
        .execute(&CapabilityApiHttpRequest {
            url: format!("{base_url}/v1/files?mode=test"),
            method: CapabilityApiMethod::Post,
            headers: vec![
                ("Authorization".to_owned(), format!("Bearer {SECRET}")),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: Some(b"{\"name\":\"demo\"}".to_vec()),
        })
        .expect("request must succeed");
    handle.join().expect("mock server must stop");

    assert_eq!(response.status, 200);
    assert_eq!(
        response.content_type.as_deref(),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(response.body, b"{\"ok\":true}");
    assert!(!response.truncated);
}

#[test]
fn truncates_oversized_responses_at_the_configured_limit() {
    let server = Server::http("127.0.0.1:0").expect("mock server must start");
    let base_url = local_url(&server);
    let body = vec![b'x'; MAX_CAPABILITY_API_RESPONSE_BYTES + 16];
    let handle = std::thread::spawn(move || {
        let request = server.recv().expect("request must arrive");
        request
            .respond(Response::from_data(body))
            .expect("response must send");
    });

    let response = UreqCapabilityApiClient::for_testing()
        .execute(&CapabilityApiHttpRequest {
            url: format!("{base_url}/large"),
            method: CapabilityApiMethod::Get,
            headers: vec![],
            body: None,
        })
        .expect("request must succeed");
    handle.join().expect("mock server must stop");

    assert_eq!(response.body.len(), MAX_CAPABILITY_API_RESPONSE_BYTES);
    assert!(response.truncated);
}

#[test]
fn rejects_redirects_instead_of_following_them() {
    let server = Server::http("127.0.0.1:0").expect("mock server must start");
    let base_url = local_url(&server);
    let handle = std::thread::spawn(move || {
        let request = server.recv().expect("request must arrive");
        let mut response = Response::empty(StatusCode(302));
        response.add_header(
            Header::from_bytes("Location", "http://127.0.0.1/other").expect("header must be valid"),
        );
        request.respond(response).expect("response must send");
    });

    let error = UreqCapabilityApiClient::for_testing()
        .execute(&CapabilityApiHttpRequest {
            url: format!("{base_url}/redirect"),
            method: CapabilityApiMethod::Get,
            headers: vec![],
            body: None,
        })
        .expect_err("redirect must be rejected");
    handle.join().expect("mock server must stop");

    assert!(matches!(error, CapabilityApiError::RedirectNotAllowed));
}

#[test]
fn rejects_oversized_request_bodies_before_transport() {
    let client = UreqCapabilityApiClient::for_testing();
    let error = client
        .execute(&CapabilityApiHttpRequest {
            url: "http://127.0.0.1:9/unused".to_owned(),
            method: CapabilityApiMethod::Post,
            headers: vec![],
            body: Some(vec![0; MAX_CAPABILITY_API_REQUEST_BODY_BYTES + 1]),
        })
        .expect_err("oversized body must fail");
    assert!(matches!(error, CapabilityApiError::Request(_)));
}

#[test]
fn production_client_requires_https_and_debug_never_prints_header_values() {
    let request = CapabilityApiHttpRequest {
        url: "http://127.0.0.1:9/private".to_owned(),
        method: CapabilityApiMethod::Get,
        headers: vec![("Authorization".to_owned(), format!("Bearer {SECRET}"))],
        body: None,
    };
    let debug = format!("{request:?}");
    assert!(!debug.contains(SECRET));

    let error = UreqCapabilityApiClient::new()
        .execute(&request)
        .expect_err("plain HTTP must be rejected");
    assert!(matches!(error, CapabilityApiError::HttpsRequired));
    assert!(!error.to_string().contains(SECRET));

    let error = UreqCapabilityApiClient::new()
        .execute(&CapabilityApiHttpRequest {
            url: "https://127.0.0.1/private".to_owned(),
            method: CapabilityApiMethod::Get,
            headers: vec![],
            body: None,
        })
        .expect_err("private HTTPS target must be rejected");
    assert!(matches!(error, CapabilityApiError::InvalidUrl));
}
