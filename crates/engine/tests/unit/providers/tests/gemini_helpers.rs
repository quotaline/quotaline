//! Builds gemini helpers fixtures for provider tests.

use super::*;

pub(super) async fn gemini_summary(quota: Value, assist: Value) -> Provider {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(assist))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .respond_with(ResponseTemplate::new(200).set_body_json(quota))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture","email":"gemini@example.com"}),
    );
    collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap()
}

pub(super) async fn gemini_reference() -> Provider {
    gemini_summary(
        serde_json::from_str(include_str!("../../../fixtures/gemini_quota.json")).unwrap(),
        serde_json::from_str(include_str!("../../../fixtures/gemini_load_code_assist.json")).unwrap(),
    )
    .await
}

pub(super) async fn gemini_malformed() -> Provider {
    gemini_summary(
        json!({"buckets":[
            {"modelId":"gemini-flash","remainingFraction":0.2,"resetTime":"not-a-date"},
            {"modelId":"gemini-flash","remainingFraction":0.8},
            {"modelId":"gemini-pro","remainingAmount":"50"},
            {"remainingFraction":0.5,"resetTime":"unknown"},
            {}
        ]}),
        json!({"currentTier":{"availableCredits":[
            {"creditType":"PROMOTIONAL","creditAmount":"2.5"},
            {"creditType":"BROKEN","creditAmount":"not-a-number"},
            {"creditAmount":"3"}
        ]}}),
    )
    .await
}

pub(super) fn gemini_oauth_fixture(home: &Path) {
    let path = home.join(".npm-global/lib/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        "const note = 'OAUTH_CLIENT_ID'; const OAUTH_CLIENT_ID = process.env.FAKE; const OAUTH_CLIENT_SECRET = ''; const OAUTH_CLIENT_ID = \"fixture-id\"; const OAUTH_CLIENT_SECRET = 'fixture-secret';",
    )
    .unwrap();
}

pub(super) async fn gemini_cached_plans(tokens: &[&str], credits: bool, expire: bool) -> Vec<String> {
    let server = MockServer::start().await;
    let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(move |_: &wiremock::Request| {
            let number = requests.fetch_add(1, Ordering::SeqCst) + 1;
            ResponseTemplate::new(200).set_body_json(json!({"currentTier":{
                "name":format!("Tier {number}"),
                "availableCredits":if credits { json!([{"creditType":"GOOGLE_ONE_AI","creditAmount":"1"}]) }
                    else { json!([]) }
            }}))
        })
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"buckets":[]})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let api = fixture_api(&server);
    let mut plans = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if index == 1 && expire {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(61)).await;
            tokio::time::resume();
        }
        credential_file(home.path(), ".gemini/oauth_creds.json", &json!({"access_token":token}));
        plans.push(collect(&api, "gemini", home.path(), false).await.unwrap().plan);
    }
    plans
}
