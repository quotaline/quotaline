//! Fetches Antigravity quota with local and cloud credentials.

use super::*;

async fn antigravity_keyring() -> Result<Option<Value>, Failure> {
    let Some(bytes) = keyring::read("Antigravity", &[("service", "gemini"), ("username", "antigravity")]).await? else {
        return Ok(None);
    };
    credentials::antigravity_document(&bytes)
        .map(Some)
        .map_err(|message| Failure::new("unreadable", message))
}

pub(super) fn antigravity_file(home: &Path) -> Result<(Value, String), Failure> {
    let mut unreadable = None;
    for path in [
        home.join(".gemini/antigravity-cli/antigravity-oauth-token"),
        app_data(home, "Antigravity").join("auth.json"),
    ] {
        match json_file(&path) {
            Ok(document) if credentials::antigravity_token(&document).is_some() => {
                return Ok((document, source(&path)));
            }
            Err(error) => {
                unreadable.get_or_insert(error);
            }
            Ok(_) => {}
        }
    }
    Err(unreadable.unwrap_or_else(|| Failure::missing_sign_in("No Antigravity sign-in found")))
}

pub(super) fn antigravity_expires_soon(document: &Value) -> bool {
    quotaline_credentials::credential_expired(
        quotaline_credentials::antigravity_credential(document).and_then(|credential| credential.expires_at),
        Timestamp::now().as_second() as f64,
    )
}

pub(super) fn antigravity_refresh_token(document: &Value) -> Option<&str> {
    quotaline_credentials::antigravity_refresh_token(document)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn antigravity_oauth_binary(path: &Path) -> Option<(String, String)> {
    quotaline_credential_sources::google_oauth::antigravity_binary(path).map(|client| (client.id, client.secret))
}

#[cfg(target_os = "macos")]
pub(super) fn antigravity_oauth_client(home: &Path) -> Option<(String, String)> {
    quotaline_macos_platform::google_oauth::antigravity_client(home, Path::new("/Applications"), &env::vars().collect())
        .map(|client| (client.id, client.secret))
}

#[cfg(target_os = "linux")]
const AGY: [&str; 1] = ["agy"];
#[cfg(windows)]
const AGY: [&str; 1] = ["agy.exe"];

#[cfg(target_os = "linux")]
fn antigravity_installs(home: &Path) -> Vec<PathBuf> {
    let binary = "resources/app/extensions/antigravity/bin/language_server_linux_x64";
    vec![
        home.join(".local/bin/agy"),
        home.join(".local/share/antigravity").join(binary),
        PathBuf::from("/usr/share/antigravity").join(binary),
        PathBuf::from("/opt/Antigravity").join(binary),
    ]
}

/// The per-user install folder Windows installers of VS Code forks use.
#[cfg(windows)]
fn antigravity_installs(home: &Path) -> Vec<PathBuf> {
    vec![
        crate::platform::config_home(home)
            .join("Programs/Antigravity/resources/app/extensions/antigravity/bin/language_server_windows_x64.exe"),
    ]
}

#[cfg(not(target_os = "macos"))]
pub(super) fn antigravity_oauth_client(home: &Path) -> Option<(String, String)> {
    if let (Ok(id), Ok(secret)) = (
        env::var("ANTIGRAVITY_OAUTH_CLIENT_ID"),
        env::var("ANTIGRAVITY_OAUTH_CLIENT_SECRET"),
    ) && !id.is_empty()
        && !secret.is_empty()
    {
        return Some((id, secret));
    }
    let mut paths = antigravity_installs(home);
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            paths.extend(AGY.map(|name| directory.join(name)));
        }
    }
    paths.into_iter().find_map(|path| antigravity_oauth_binary(&path))
}

pub(super) async fn refresh_antigravity(client: &Api, home: &Path, stored: &Value) -> Result<Value, Failure> {
    let refresh_token = antigravity_refresh_token(stored)
        .ok_or_else(|| Failure::new("sign-in", "Antigravity refresh token is missing; sign in again."))?;
    check_oauth_rejection(client, "antigravity", stored).await?;
    let (id, secret) = antigravity_oauth_client(home)
        .ok_or_else(|| Failure::new("unreadable", "Cannot find the installed Antigravity OAuth client"))?;
    let response = oauth_request(
        client,
        "antigravity",
        stored,
        client
            .http
            .post(&client.gemini_token)
            .header("Accept", "application/json")
            .form(&quotaline_credentials::google_refresh_request(
                &id,
                &secret,
                refresh_token,
            )),
    )
    .await?;
    let provider = quotaline_credentials::RefreshProvider::Antigravity;
    quotaline_credentials::complete_refresh(provider, stored, &response, Timestamp::now().as_millisecond(), "")
        .map_err(|error| Failure::new("unreadable", error.message(provider)))
}

pub(super) async fn antigravity_cloud(client: &Api, token: &str) -> Result<(Value, String), Failure> {
    let bases = client.antigravity_bases.each_ref().map(String::as_str);
    let home = crate::platform::home_text();
    core_antigravity::fetch::fetch_quota(
        client,
        &bases,
        token,
        &format!("Run {}.", login("antigravity")),
        home.as_deref(),
    )
    .await
    .result
    .map_err(|error| pipeline_failure(error, "antigravity"))
}

pub(super) async fn antigravity_tier(client: &Api, base: &str, token: &str) -> Result<Option<Value>, String> {
    let mut cache = client.antigravity_tier_cache.lock().await;
    if let Some(tier) = cache.read(token, client.cache_epoch.elapsed().as_secs_f64()) {
        return Ok(tier);
    }
    let plan =
        core_antigravity::fetch::fetch_plan(client, base, token, crate::platform::home_text().as_deref()).await?;
    let tier = plan["currentTier"].as_object().map(|_| plan["currentTier"].clone());
    cache.record(token, tier.clone(), client.cache_epoch.elapsed().as_secs_f64());
    Ok(tier)
}

pub(super) async fn antigravity(client: &Api, home: &Path, allow_token_refresh: bool) -> Result<Provider, Failure> {
    if client.antigravity_local_enabled
        && client.antigravity_auth_file.is_none()
        && let Some((data, status, source)) = antigravity_local::quota().await
    {
        return antigravity_provider(&data, &status, source.into());
    }
    let explicit = client.antigravity_auth_file.clone();
    let use_file = explicit.is_some();
    let (doc, source) = if use_file {
        let path = explicit.unwrap();
        (json_file(&path)?, source(&path))
    } else {
        match if client.allow_keyring {
            antigravity_keyring().await
        } else {
            Ok(None)
        } {
            Ok(Some(document)) => (document, format!("{}: gemini/antigravity", keyring::STORE)),
            Ok(None) => antigravity_file(home)?,
            Err(error) => {
                let Ok(file) = antigravity_file(home) else {
                    return Err(error);
                };
                file
            }
        }
    };
    let mut active = {
        let cache = client.antigravity_refreshed.lock().await;
        cache
            .as_ref()
            .filter(|(stored, _)| stored == &doc)
            .map(|(_, active)| active.clone())
            .unwrap_or_else(|| doc.clone())
    };
    let mut refreshed = false;
    if antigravity_expires_soon(&active) {
        if !allow_token_refresh {
            return Err(Failure::new(
                "sign-in",
                "Antigravity token expired. Enable token refresh or sign in again.",
            )
            .with_expired());
        }
        active = refresh_antigravity(client, home, &doc)
            .await
            .map_err(Failure::with_expired)?;
        *client.antigravity_refreshed.lock().await = Some((doc.clone(), active.clone()));
        refreshed = true;
    }
    let (data, base) = loop {
        let token = credential(credentials::antigravity_token(&active), "antigravity")?;
        match antigravity_cloud(client, token).await {
            Ok(data) => break data,
            Err(error) if error.status == "sign-in" && allow_token_refresh && !refreshed => {
                active = refresh_antigravity(client, home, &doc).await?;
                *client.antigravity_refreshed.lock().await = Some((doc.clone(), active.clone()));
                refreshed = true;
            }
            Err(error) => return Err(error.with_credential()),
        }
    };
    let token = credential(credentials::antigravity_token(&active), "antigravity")?;
    let tier = antigravity_tier(client, &base, token).await;
    let mut provider = antigravity_provider(&data, &doc, source).map_err(Failure::with_credential)?;
    match tier {
        Ok(Some(tier)) => {
            if let Some(name) = text(&tier["name"]) {
                provider.plan = name.into();
            }
        }
        Err(message) => provider.warnings.push(format!("Plan details unavailable: {message}")),
        Ok(None) => {}
    }
    Ok(provider)
}

pub(super) fn antigravity_provider(data: &Value, identity: &Value, source: String) -> Result<Provider, Failure> {
    core_antigravity::project(data, core_antigravity::identity(identity), source).map_err(|error| match error {
        core_antigravity::QuotaError::Unauthenticated => Failure::new("sign-in", "Antigravity session expired"),
        _ => Failure::new("error", "Antigravity quota request failed"),
    })
}
