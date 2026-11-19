//! Fetches Claude usage and OAuth profile data.

use super::*;
use quotaline_claude as claude;

/// Where Claude Code keeps the sign-in this read found.
pub(super) enum Saved {
    File(PathBuf),
    #[cfg(target_os = "macos")]
    Keychain {
        service: String,
        account: Option<String>,
    },
}

impl Saved {
    fn source(&self) -> String {
        match self {
            Self::File(path) => source(path),
            #[cfg(target_os = "macos")]
            Self::Keychain { service, .. } => format!("{}: {service}", keyring::STORE),
        }
    }
}

/// The sign-in as read, held to check nothing replaced it before a refresh writes back.
enum Original {
    File(quotaline_storage::CredentialFileGuard),
    #[cfg(target_os = "macos")]
    Keychain {
        service: String,
        account: Option<String>,
        document: Value,
    },
}

impl Original {
    fn observe(saved: &Saved, document: &Value) -> Result<Self, Failure> {
        Ok(match saved {
            Saved::File(path) => Self::File(
                quotaline_storage::CredentialFileGuard::observe(path, document)
                    .map_err(|error| credential_file_failure("Claude", error))?,
            ),
            #[cfg(target_os = "macos")]
            Saved::Keychain { service, account } => Self::Keychain {
                service: service.clone(),
                account: account.clone(),
                document: document.clone(),
            },
        })
    }

    async fn replace(self, document: &Value) -> Result<(), Failure> {
        match self {
            Self::File(guard) => guard
                .replace(document)
                .map_err(|error| credential_file_failure("Claude", error)),
            #[cfg(target_os = "macos")]
            Self::Keychain {
                service,
                account,
                document: expected,
            } => {
                let mut attributes = vec![("service", service.as_str())];
                attributes.extend(account.as_deref().map(|account| ("username", account)));
                keyring::replace("Claude", &attributes, &expected, document).await
            }
        }
    }
}

/// Claude Code keeps its macOS sign-in in the Keychain, and writes the file only when the Keychain refuses it.
#[cfg(target_os = "macos")]
async fn claude_saved(root: &Path) -> Result<(Value, Saved), Failure> {
    let directory = env::var("CLAUDE_CONFIG_DIR").ok();
    let service = quotaline_credentials::claude_keychain_service(directory.as_deref());
    if let Some(item) = keyring::load("Claude", &service, None).await? {
        let document = serde_json::from_slice(&item.data)
            .map_err(|_| Failure::new("unreadable", "Claude Keychain item is not JSON"))?;
        return Ok((
            document,
            Saved::Keychain {
                service,
                account: item.account,
            },
        ));
    }
    let path = root.join(".credentials.json");
    Ok((json_file(&path)?, Saved::File(path)))
}

#[cfg(not(target_os = "macos"))]
async fn claude_saved(root: &Path) -> Result<(Value, Saved), Failure> {
    let path = root.join(".credentials.json");
    Ok((json_file(&path)?, Saved::File(path)))
}

pub(super) async fn claude(client: &Api, home: &Path, allow_token_refresh: bool) -> Result<Provider, Failure> {
    let root = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let (mut doc, saved) = claude_saved(&root).await?;
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
        doc = refresh_claude(client, &saved, doc)
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
        saved.source(),
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
    let result = claude::fetch::fetch_profile(
        client,
        &client.claude_profile,
        token,
        crate::platform::home_text().as_deref(),
    )
    .await;
    let resolved = cache.record_attempt(&key, result, client.cache_epoch.elapsed().as_secs_f64());
    (resolved.value, resolved.warning)
}

pub(super) async fn refresh_claude(client: &Api, saved: &Saved, mut document: Value) -> Result<Value, Failure> {
    let original = Original::observe(saved, &document)?;
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
    original.replace(&document).await?;
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
