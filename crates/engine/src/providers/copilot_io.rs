//! Fetches GitHub Copilot quota responses.

use super::*;
use quotaline_copilot::request as copilot_request;
use quotaline_credential_sources::{
    CopilotDiscovery, CopilotDiscoveryError, CopilotLegacyRecovery, CopilotOrigin, CopilotSourceError,
    begin_copilot_discovery,
};
use quotaline_credentials::CopilotCredential;

pub(super) fn copilot_secret(bytes: &[u8], account: &str) -> Result<Option<CopilotCredential>, Failure> {
    use quotaline_credentials::CopilotSecretError;
    quotaline_credentials::copilot_keyring_credential(bytes, Some(account)).map_err(|error| {
        Failure::new(
            "unreadable",
            match error {
                CopilotSecretError::InvalidText => "Copilot keyring item is not text",
                CopilotSecretError::InvalidJson => "Copilot keyring item is not JSON",
                CopilotSecretError::InvalidHost => "Invalid Copilot keyring host",
            },
        )
    })
}

pub(super) async fn copilot_keyring(accounts: &[String]) -> Result<Option<CopilotCredential>, Failure> {
    let Some(service) = keyring::connect().await else {
        return Ok(None);
    };
    let found = keyring::search("Copilot", &service, &[("service", "copilot-cli")]).await?;
    let mut items = Vec::new();
    for item in &found.unlocked {
        let attributes = keyring::attributes("Copilot", item).await?;
        let account = attributes
            .get("account")
            .or_else(|| attributes.get("username"))
            .cloned()
            .unwrap_or_default();
        let rank = accounts
            .iter()
            .position(|candidate| candidate == &account)
            .unwrap_or(usize::MAX);
        if accounts.is_empty() || rank != usize::MAX {
            items.push((rank, account, item));
        }
    }
    items.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    for (_, account, item) in items {
        if let Some(credentials) = copilot_secret(&keyring::secret("Copilot", item).await?, &account)? {
            return Ok(Some(credentials));
        }
    }
    for item in &found.locked {
        let attributes = keyring::attributes("Copilot", item).await?;
        if accounts.is_empty()
            || attributes
                .get("account")
                .or_else(|| attributes.get("username"))
                .is_some_and(|account| accounts.contains(account))
        {
            return Err(keyring::locked("Copilot"));
        }
    }
    Ok(None)
}

pub(super) async fn copilot(client: &Api, home: &Path) -> Result<Provider, Failure> {
    let environment_credential = client
        .allow_environment_credentials
        .then(|| {
            let environment = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN", "GH_HOST"]
                .into_iter()
                .filter_map(|key| env::var(key).ok().map(|value| (key.to_string(), value)))
                .collect();
            quotaline_credentials::copilot_environment_credential(&environment)
        })
        .flatten();
    let path = client
        .allow_environment_credentials
        .then(|| env::var_os("COPILOT_HOME").map(PathBuf::from))
        .flatten()
        .unwrap_or_else(|| home.join(".copilot"))
        .join("config.json");
    let found = match begin_copilot_discovery(environment_credential, &path) {
        CopilotDiscovery::Found(found) => Some(found),
        CopilotDiscovery::Pending(pending) => {
            let keyring = if client.allow_keyring {
                copilot_keyring(pending.keyring_accounts()).await
            } else {
                Ok(None)
            };
            pending
                .finish(
                    keyring,
                    &[
                        config_home(home).join("github-copilot/hosts.json"),
                        config_home(home).join("github-copilot/apps.json"),
                    ],
                    CopilotLegacyRecovery::TryNext,
                )
                .map_err(|error| match error {
                    CopilotDiscoveryError::Keyring(error) => error,
                    CopilotDiscoveryError::Source(error) => copilot_source_failure(error),
                })?
        }
    }
    .ok_or_else(|| Failure::missing_sign_in(format!("No sign-in found. Run {}.", login("copilot"))))?;
    let host = quotaline_credentials::copilot_host(&found.credential.host)
        .ok_or_else(|| Failure::new("unreadable", "Invalid GitHub host"))?;
    let url = if host == "github.com" {
        client.copilot_usage.clone()
    } else {
        copilot_request::user_url(&host)
    };
    #[cfg(feature = "fixture-api")]
    let url = if let Some(base) = &client.fixture_base {
        let path = reqwest::Url::parse(&url)
            .map_err(|_| Failure::new("unreadable", "Invalid GitHub host"))?
            .path()
            .to_owned();
        format!("{base}{path}")
    } else {
        url
    };
    let data = copilot::fetch::fetch_user(client, &url, &found.credential.token)
        .await
        .map_err(|error| pipeline_failure(error, "copilot").with_credential())?;
    let source = match found.origin {
        CopilotOrigin::Environment => "environment".into(),
        CopilotOrigin::Keyring => "Secret Service: copilot-cli".into(),
        CopilotOrigin::Cli(path) | CopilotOrigin::Legacy(path) => path.to_string_lossy().into(),
    };
    let mut provider = copilot_provider(&data, source);
    if let Some(user) = found.credential.user.filter(|user| !user.is_empty()) {
        provider.identity = user;
    }
    Ok(provider)
}

fn copilot_source_failure(error: CopilotSourceError) -> Failure {
    match error {
        CopilotSourceError::File { path, .. } => Failure::new("unreadable", format!("Cannot read {}", path.display())),
        CopilotSourceError::Json { path, .. } | CopilotSourceError::LegacyShape { path } => {
            Failure::new("unreadable", format!("{} is not valid JSON", path.display()))
        }
    }
}

pub(super) fn copilot_provider(data: &Value, source: String) -> Provider {
    copilot::project(data, source, text(&data["login"]).unwrap_or("").into())
}
