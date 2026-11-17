//! Fetches Gemini CLI quota with local credentials.

use super::*;
use quotaline_gemini::request as gemini_request;
use quotaline_http::transport::header_list;

const GEMINI_KEYRING: &[(&str, &str)] = &[("service", "gemini-cli-oauth"), ("username", "main-account")];

pub(super) fn gemini_oauth_client(home: &Path) -> Option<(String, String)> {
    if let (Ok(id), Ok(secret)) = (
        env::var("GEMINI_OAUTH_CLIENT_ID"),
        env::var("GEMINI_OAUTH_CLIENT_SECRET"),
    ) && !id.is_empty()
        && !secret.is_empty()
    {
        return Some((id, secret));
    }
    let mut roots = Vec::new();
    if let Some(prefix) = env::var_os("NPM_CONFIG_PREFIX") {
        roots.push(PathBuf::from(prefix));
    }
    roots.extend([
        home.join(".npm-global"),
        PathBuf::from("/usr/local"),
        PathBuf::from("/usr"),
    ]);
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            if let Ok(executable) = directory.join("gemini").canonicalize() {
                roots.extend(executable.ancestors().take(8).map(Path::to_path_buf));
            }
        }
    }
    const SOURCES: [&str; 4] = [
        "node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js",
        "lib/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js",
        "lib/node_modules/@google/gemini-cli/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js",
        "lib/node_modules/@google/gemini-cli/dist/gemini.js",
    ];
    for root in roots {
        for relative in SOURCES {
            if let Ok(source) = fs::read_to_string(root.join(relative))
                && let Some(client) = quotaline_credential_sources::google_oauth::gemini_source(&source)
            {
                return Some((client.id, client.secret));
            }
        }
    }
    None
}

pub(super) async fn gemini(client: &Api, home: &Path, allow_token_refresh: bool) -> Result<Provider, Failure> {
    let path = env::var_os("GEMINI_CLI_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.into())
        .join(".gemini/oauth_creds.json");
    let (mut doc, source, from_keyring) = match if client.allow_keyring
        && quotaline_credentials::gemini_storage_mode(env::var("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE").ok().as_deref())
            == quotaline_credentials::GeminiStorage::Keychain
    {
        keyring::read_json("Gemini", GEMINI_KEYRING).await
    } else {
        Ok(None)
    } {
        Ok(Some(document)) => (document, "Secret Service: gemini-cli-oauth".into(), true),
        Ok(None) => (json_file(&path)?, source(&path), false),
        Err(error) => {
            let document = match json_file(&path) {
                Ok(document) => document,
                Err(_) => return Err(error),
            };
            if quotaline_credentials::gemini_credential_document(&document).is_none() {
                return Err(error);
            }
            (document, source(&path), false)
        }
    };
    let mut credential_document = quotaline_credentials::gemini_credential_document(&doc);
    if quotaline_credentials::credential_expired(
        credential_document
            .as_ref()
            .and_then(|credential| credential.expires_at),
        Timestamp::now().as_millisecond() as f64 / 1000.0,
    ) {
        if !allow_token_refresh {
            return Err(Failure::new(
                "sign-in",
                "Gemini token expired. Enable token refresh or sign in with Gemini CLI.",
            )
            .with_expired());
        }
        doc = refresh_gemini(client, home, &path, doc, from_keyring)
            .await
            .map_err(Failure::with_expired)?;
        credential_document = quotaline_credentials::gemini_credential_document(&doc);
    }
    let token = credential(
        credential_document
            .as_ref()
            .map(|credential| credential.access_token.as_str()),
        "gemini",
    )?;
    let fingerprint = hex::encode(quotaline_credentials::gemini_cache_fingerprint(
        token,
        text(&doc["refresh_token"]).or_else(|| text(&doc["token"]["refreshToken"])),
        text(&doc["id_token"]).or_else(|| text(&doc["token"]["idToken"])),
    ));
    let hint = format!("Run {}.", login("gemini"));
    let home = env::var("HOME").ok();
    let fetch = core_gemini::fetch::fetch(
        client,
        &client.gemini_assist_cache,
        core_gemini::fetch::Context {
            base: &client.gemini_base,
            headers: header_list(gemini_request::headers(token, "linux", None)),
            fingerprint: &fingerprint,
            now: client.cache_epoch.elapsed().as_secs_f64(),
            hint: &hint,
            home: home.as_deref(),
        },
    )
    .await;
    let (quota, assist) = match fetch.outcome {
        core_gemini::fetch::Outcome::Quota { quota, assist } => (quota, assist),
        core_gemini::fetch::Outcome::Unsupported(reason) => {
            return Err(Failure::unsupported(reason).with_credential());
        }
        core_gemini::fetch::Outcome::Failure(error) => {
            return Err(pipeline_failure(error, "gemini").with_credential());
        }
    };
    let mut provider = gemini_provider(&quota, &assist, &doc, source);
    provider.warnings.extend(fetch.warnings);
    Ok(provider)
}

pub(super) async fn refresh_gemini(
    client: &Api,
    home: &Path,
    path: &Path,
    document: Value,
    from_keyring: bool,
) -> Result<Value, Failure> {
    let expected = document.clone();
    let original = if from_keyring {
        None
    } else {
        Some(
            quotaline_storage::CredentialFileGuard::observe(path, &document)
                .map_err(|error| credential_file_failure("Gemini", error))?,
        )
    };
    let refresh_token = text(&document["refresh_token"])
        .or_else(|| text(&document["token"]["refreshToken"]))
        .ok_or_else(|| Failure::new("sign-in", "Gemini refresh token is missing; sign in with Gemini CLI."))?;
    check_oauth_rejection(client, "gemini", &document).await?;
    let (id, secret) = gemini_oauth_client(home)
        .ok_or_else(|| Failure::new("unreadable", "Cannot find the installed Gemini CLI OAuth client"))?;
    let response = oauth_request(
        client,
        "gemini",
        &document,
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
    let provider = quotaline_credentials::RefreshProvider::Gemini;
    let document =
        quotaline_credentials::complete_refresh(provider, &document, &response, Timestamp::now().as_millisecond(), "")
            .map_err(|error| Failure::new("unreadable", error.message(provider)))?;
    if let Some(original) = original {
        original
            .replace(&document)
            .map_err(|error| credential_file_failure("Gemini", error))?;
    } else {
        keyring::replace("Gemini", GEMINI_KEYRING, &expected, &document).await?;
    }
    Ok(document)
}

pub(super) fn gemini_provider(quota: &Value, assist: &Value, credential: &Value, source: String) -> Provider {
    let claims = text(&credential["id_token"])
        .or_else(|| text(&credential["token"]["idToken"]))
        .and_then(credentials::jwt_claims)
        .unwrap_or(Value::Null);
    let organization = text(&claims["hd"]).unwrap_or("");
    core_gemini::project(
        quota,
        assist,
        (!organization.is_empty()).then_some(organization),
        text(&claims["email"])
            .or_else(|| text(&credential["email"]))
            .unwrap_or("")
            .into(),
        source,
    )
}
