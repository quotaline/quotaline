//! Sends shared pipeline requests through reqwest against a local server.

use super::*;
use quotaline_http::HttpError;
use quotaline_http::transport::Body;
use quotaline_http::transport::Request;
use quotaline_http::transport::Transport;

#[tokio::test]
async fn transport_posts_forms_with_a_timeout() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string("grant_type=refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&server)
        .await;
    let request = Request {
        body: Body::Form(vec![("grant_type".into(), "refresh".into())]),
        timeout_millis: Some(5_000),
        ..Request::get(format!("{}/token", server.uri()), Vec::new(), "test.form")
    };
    assert_eq!(fixture_api(&server).send(request).await.unwrap(), b"ok");
}

#[tokio::test]
async fn transport_reports_a_vendor_security_check() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .respond_with(ResponseTemplate::new(403).insert_header("Content-Type", "text/html"))
        .mount(&server)
        .await;
    let error = fixture_api(&server)
        .send(Request::get(server.uri(), Vec::new(), "test.get"))
        .await
        .unwrap_err();
    assert_eq!(error, HttpError::VendorSecurityCheck(403));
}

#[tokio::test]
async fn transport_reports_an_unreachable_server() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let server = MockServer::start().await;
    let error = fixture_api(&server)
        .send(Request::get(url, Vec::new(), "test.get"))
        .await
        .unwrap_err();
    assert_eq!(pipeline_failure(error, "cursor").status, "offline");
}

#[test]
fn pipeline_failures_ask_for_sign_in_with_the_login_command() {
    let failure = pipeline_failure(
        HttpError::Response {
            status: 401,
            body: String::new(),
            retry_after: None,
        },
        "cursor",
    );
    assert_eq!(
        (failure.status, failure.message.as_str()),
        ("sign-in", "HTTP 401. Run cursor-agent login.")
    );
}

#[test]
fn pipeline_failures_keep_the_provider_rate_limit_delay() {
    let failure = pipeline_failure(
        HttpError::Response {
            status: 429,
            body: String::new(),
            retry_after: Some(2.5),
        },
        "cursor",
    );
    assert_eq!((failure.status, failure.retry_after), ("rate-limited", 3));
}
