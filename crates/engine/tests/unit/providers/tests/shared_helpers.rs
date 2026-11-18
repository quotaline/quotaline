//! Builds shared helpers fixtures for provider tests.

use super::*;

pub(super) fn fixture_api(server: &MockServer) -> Api {
    let mut api = Api::new().unwrap();
    let base = server.uri();
    api.claude_usage = format!("{base}/api/oauth/usage");
    api.claude_profile = format!("{base}/api/oauth/profile");
    api.claude_token = format!("{base}/v1/oauth/token");
    api.codex_base = format!("{base}/backend-api");
    api.codex_token = format!("{base}/oauth/token");
    api.gemini_base = base.clone();
    api.gemini_token = format!("{base}/token");
    api.antigravity_bases = [base.clone(), base.clone()];
    api.antigravity_local_enabled = false;
    api.cursor_usage = format!("{base}/api/usage-summary");
    api.cursor_period = format!("{base}/aiserver.v1.DashboardService/GetCurrentPeriodUsage");
    api.cursor_me = format!("{base}/api/auth/me");
    api.copilot_usage = format!("{base}/copilot_internal/user");
    #[cfg(feature = "fixture-api")]
    {
        api.fixture_base = Some(base);
    }
    api.allow_environment_credentials = false;
    api.allow_keyring = false;
    api
}

pub(super) fn credential_file(home: &Path, relative: &str, document: &Value) {
    let path = home.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, document.to_string()).unwrap();
}
