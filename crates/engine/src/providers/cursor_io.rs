//! Fetches Cursor plan, team, and agent usage.

use super::*;
use quotaline_credential_sources::{CursorOrigin, CursorSourceError, discover_cursor};

pub(super) fn cursor_credentials(home: &Path) -> Result<(String, Value, PathBuf), Failure> {
    let state_path = app_data(home, "Cursor").join("User/globalStorage/state.vscdb");
    let agent_path = home.join(".cursor/auth.json");
    let found = discover_cursor(&state_path, &agent_path).map_err(|error| {
        let message = match error {
            CursorSourceError::Query(_) => "Cannot query Cursor state".into(),
            CursorSourceError::Open(_) | CursorSourceError::Read(_) => "Cannot read Cursor state".into(),
            CursorSourceError::Json(_) => format!("{} is not valid JSON", agent_path.display()),
            CursorSourceError::File(_) => format!("Cannot read {}", agent_path.display()),
        };
        Failure::new("unreadable", message)
    })?;
    let found = found.ok_or_else(|| Failure::missing_sign_in(format!("No sign-in found. Run {}.", login("cursor"))))?;
    let path = match found.origin {
        CursorOrigin::App => state_path,
        CursorOrigin::Agent => agent_path,
    };
    let document = json!({"email": found.auth.email, "membershipType": found.auth.membership_type});
    Ok((found.auth.access_token, document, path))
}

pub(super) async fn cursor(client: &Api, home: &Path) -> Result<Provider, Failure> {
    let (token, doc, path) = cursor_credentials(home)?;
    if quotaline_credentials::credential_expired(
        quotaline_credentials::jwt_expiry(&token),
        Timestamp::now().as_second() as f64,
    ) {
        return Err(Failure::new(
            "sign-in",
            "Cursor session expired; open Cursor or run cursor-agent login.",
        )
        .with_expired());
    }
    let cookie = quotaline_credentials::cursor_session_cookie(&token);
    let usage = cursor::fetch::fetch_usage(
        client,
        &cursor::fetch::Endpoints {
            summary: &client.cursor_usage,
            period: &client.cursor_period,
        },
        &cookie,
        &token,
        crate::platform::home_text().as_deref(),
    )
    .await
    .map_err(|error| pipeline_failure(error, "cursor").with_credential())?;
    let mut provider = cursor_provider(&usage.summary, &doc, source(&path), usage.period_message.as_deref());
    if let Some(warning) = usage.warning {
        provider.warnings.push(warning);
    }
    if let Some(identity) = cursor_identity(client, &token, &cookie).await {
        provider.identity = identity;
    }
    Ok(provider)
}

pub(super) async fn cursor_identity(client: &Api, token: &str, cookie: &str) -> Option<String> {
    let mut cache = client.cursor_identity.lock().await;
    if let Some(identity) = cache.get(token, client.cache_epoch.elapsed().as_secs_f64()) {
        return identity;
    }
    let identity = cursor::fetch::fetch_email(client, &client.cursor_me, cookie).await;
    cache.remember(token, identity.clone(), client.cache_epoch.elapsed().as_secs_f64());
    identity
}

pub(super) fn cursor_provider(
    data: &Value,
    credential: &Value,
    source: String,
    period_message: Option<&str>,
) -> Provider {
    cursor::project(
        data,
        text(&credential["membershipType"]),
        text(&credential["email"]),
        None,
        period_message,
        source,
    )
}
