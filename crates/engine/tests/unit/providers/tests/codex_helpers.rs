//! Builds codex helpers fixtures for provider tests.

use super::*;

pub(super) async fn codex_summary(data: Value) -> Provider {
    let server = MockServer::start().await;
    let resets = match &data["rate_limit_reset_credits"] {
        Value::Null => json!({}),
        reported => reported.clone(),
    };
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(data))
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(resets))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"codex-fixture"}}),
    );
    collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap()
}

pub(super) async fn codex_reset_readings(
    readings: &[f64],
    reset: Option<i64>,
    accounts: &[&str],
) -> Vec<(f64, String)> {
    let server = MockServer::start().await;
    let index = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let samples = readings.to_vec();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(move |_: &wiremock::Request| {
            let at = index.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({
                "rate_limit":{"primary_window":{"used_percent":samples[at],
                    "limit_window_seconds":18000,"reset_at":reset}}
            }))
        })
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let api = fixture_api(&server);
    let mut observed = Vec::new();
    for account in accounts {
        credential_file(
            home.path(),
            ".codex/auth.json",
            &json!({"tokens":{"access_token":"codex-fixture","account_id":account}}),
        );
        let provider = collect(&api, "codex", home.path(), false).await.unwrap();
        observed.push((provider.windows[0].used, provider.windows[0].detail.clone()));
    }
    observed
}

pub(super) fn covered_endpoints(analytics: Option<CodexAnalytics>) -> usize {
    analytics
        .and_then(|analytics| analytics.snapshot)
        .map_or(0, |snapshot| snapshot.covered_scopes.len())
}
