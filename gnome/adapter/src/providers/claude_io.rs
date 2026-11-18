//! Fetches Claude usage and OAuth profile data.

use super::*;
use quotaline_claude as claude;

pub(super) async fn claude(client: &Api, home: &Path, allow_token_refresh: bool) -> Result<Provider, Failure> {
    let root = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let path = root.join(".credentials.json");
    let mut doc = json_file(&path)?;
    let mut credential_document = quotaline_credentials::claude_credential_document(&doc);
    if quotaline_credentials::credential_expired(
        credential_document
            .as_ref()
            .and_then(|credential| credential.expires_at),
        Timestamp::now().as_millisecond() as f64 / 1000.0,
    ) {
        if !allow_token_refresh {
            return Err(Failure::new(
                "sign-in",
                "Claude token expired. Enable token refresh or sign in with Claude Code.",
            )
            .with_expired());
        }
        doc = refresh_claude(client, &path, doc)
            .await
            .map_err(Failure::with_expired)?;
        credential_document = quotaline_credentials::claude_credential_document(&doc);
    }
    let auth = &doc["claudeAiOauth"];
    let token = credential(
        credential_document
            .as_ref()
            .map(|credential| credential.access_token.as_str()),
        "claude",
    )?;
    let (usage, (profile, profile_warning)) = tokio::join!(
        claude::fetch::fetch_usage(client, &client.claude_usage, token),
        claude_profile(client, token)
    );
    let mut provider = claude_provider(
        &usage.map_err(|error| pipeline_failure(error, "claude").with_credential())?,
        auth,
        profile.as_ref(),
        source(&path),
    )
    .map_err(Failure::with_credential)?;
    if !credential_document.is_some_and(|credential| credential.has_profile_scope) {
        provider
            .warnings
            .push("Token lacks the user:profile scope; sign in with `claude` rather than `claude setup-token`.".into());
    }
    if let Some(warning) = profile_warning {
        provider.warnings.push(warning);
    }
    Ok(provider)
}

pub(super) async fn claude_profile(client: &Api, token: &str) -> (Option<Value>, Option<String>) {
    let key = hex::encode(Sha256::digest(token.as_bytes()));
    let mut cache = client.claude_profile_cache.lock().await;
    let cached = cache.read(&key, client.cache_epoch.elapsed().as_secs_f64());
    if !cached.should_fetch {
        return (cached.value, cached.warning);
    }
    let result =
        claude::fetch::fetch_profile(client, &client.claude_profile, token, env::var("HOME").ok().as_deref()).await;
    let resolved = cache.record_attempt(&key, result, client.cache_epoch.elapsed().as_secs_f64());
    (resolved.value, resolved.warning)
}

pub(super) async fn refresh_claude(client: &Api, path: &Path, mut document: Value) -> Result<Value, Failure> {
    let original = quotaline_storage::CredentialFileGuard::observe(path, &document)
        .map_err(|error| credential_file_failure("Claude", error))?;
    let refresh_token = text(&document["claudeAiOauth"]["refreshToken"])
        .ok_or_else(|| Failure::new("sign-in", "Claude refresh token is missing; sign in with Claude Code."))?;
    let response = oauth_request(
        client,
        "claude",
        &document,
        client
            .http
            .post(&client.claude_token)
            .header("Accept", "application/json")
            .header("User-Agent", quotaline_credentials::CLAUDE_USER_AGENT)
            .json(&quotaline_credentials::claude_refresh_request(refresh_token)),
    )
    .await?;
    let provider = quotaline_credentials::RefreshProvider::Claude;
    document =
        quotaline_credentials::complete_refresh(provider, &document, &response, Timestamp::now().as_millisecond(), "")
            .map_err(|error| Failure::new("unreadable", error.message(provider)))?;
    original
        .replace(&document)
        .map_err(|error| credential_file_failure("Claude", error))?;
    Ok(document)
}

pub(super) fn claude_provider(
    data: &Value,
    auth: &Value,
    profile: Option<&Value>,
    source: String,
) -> Result<Provider, Failure> {
    quotaline_claude::project(quotaline_claude::Input {
        data,
        auth,
        profile,
        source,
    })
    .map_err(|_| {
        pipeline_failure(
            quotaline_http::HttpError::Decoding("claude.usage: invalid payload".into()),
            "claude",
        )
    })
}
