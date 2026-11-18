//! Fetches Codex quota and analytics responses.

use super::*;

pub(super) async fn codex(client: &Api, home: &Path, allow_token_refresh: bool) -> Result<Provider, Failure> {
    let configured = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let discovered = discover_codex_document(client, &configured, home).await;
    let root = discovered
        .as_ref()
        .map_or_else(|_| configured.clone(), |(root, _, _, _, _)| root.clone());
    let root = client.codex_selected_home.lock().await.clone().unwrap_or(root);
    let subscription_active_until = discovered
        .as_ref()
        .ok()
        .and_then(|(_, document, _, _, _)| codex_subscription_until(document));
    let remote = match discovered {
        Ok((_, document, source, from_keyring, account_selection)) => {
            codex_remote(
                client,
                &root,
                allow_token_refresh,
                document,
                source,
                from_keyring,
                account_selection,
            )
            .await
        }
        Err(error) => Err(error),
    };
    match remote {
        Ok(provider) => Ok(provider),
        Err(error) if !quotaline_codex::local_fallback(error.kind()) => Err(error),
        Err(error) => {
            let throttled = client.local_scan_throttled.load(Ordering::Relaxed);
            let cache = Arc::clone(&client.codex_rollout_cache);
            let reading = tokio::task::spawn_blocking(move || cache.latest(&root, throttled))
                .await
                .map_err(|_| Failure::new("error", "Codex rollout task failed"))?
                .map_err(|_| Failure::new("unreadable", "Cannot read Codex rollouts"))?;
            let Some(reading) = reading else {
                return Err(error);
            };
            let mut provider = codex_provider(&reading.data, source(&reading.path));
            provider.status = "stale".into();
            provider.notice = error.message;
            provider
                .warnings
                .push("Showing the last values Codex CLI logged locally.".into());
            provider.updated = reading.observed;
            provider.retry_after = error.retry_after;
            provider.credential_usable = error.credential_usable;
            provider.credential_health = error.credential_health;
            provider.subscription_active_until = subscription_active_until;
            Ok(provider)
        }
    }
}

pub(super) fn codex_subscription_until(document: &Value) -> Option<String> {
    text(&document["tokens"]["id_token"]).and_then(quotaline_credentials::codex_subscription_until)
}

pub(super) async fn discover_codex_document(
    client: &Api,
    configured: &Path,
    home: &Path,
) -> Result<(PathBuf, Value, String, bool, Option<quotaline_quota::AccountSelection>), Failure> {
    let mut selected = client.codex_selected_home.lock().await;
    let mut first_error = None;
    let mut accounts = Vec::new();
    for root in quotaline_credential_sources::codex_homes(configured, home) {
        match codex_document(client, &root).await {
            Ok((document, source, from_keyring)) if quotaline_credentials::codex_credential_available(&document) => {
                accounts.push((root, document, source, from_keyring));
            }
            Ok(_) => {
                first_error.get_or_insert_with(|| Failure::missing_sign_in("Codex credentials have no token"));
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    if accounts.is_empty() {
        return Err(first_error.expect("configured Codex home was checked"));
    }
    let homes = accounts.iter().map(|(root, _, _, _)| root.clone()).collect::<Vec<_>>();
    let chosen = quotaline_credential_sources::select_codex_home(selected.as_deref(), &homes).ok_or_else(|| {
        Failure::missing_sign_in("The selected Codex account is unavailable; sign in with Codex CLI.")
    })?;
    if selected.is_none() {
        *selected = Some(chosen.clone());
    }
    let count = accounts.len();
    let (root, document, source, from_keyring) = accounts
        .into_iter()
        .find(|(root, _, _, _)| root == &chosen)
        .expect("selected Codex home is in the available accounts");
    let account_selection = if count > 1 {
        let account = text(&document["tokens"]["id_token"])
            .and_then(credentials::jwt_claims)
            .and_then(|claims| text(&claims["email"]).map(str::to_string));
        let label = quotaline_credential_sources::codex_account_label(account.as_deref(), &root, home);
        Some(quotaline_quota::AccountSelection {
            count: count as u32,
            label,
        })
    } else {
        None
    };
    Ok((root, document, source, from_keyring, account_selection))
}

pub(super) async fn codex_document(client: &Api, root: &Path) -> Result<(Value, String, bool), Failure> {
    let path = root.join("auth.json");
    let file = json_file(&path);
    let mode = quotaline_credential_sources::read_codex_credential_store_mode(&root.join("config.toml"));
    let keyring = if !client.allow_keyring
        || mode == Some(quotaline_credential_sources::CodexCredentialStoreMode::File)
        || (mode.is_none() && file.as_ref().is_ok_and(|doc| !doc.is_null()))
    {
        None
    } else {
        let account = credentials::codex_key(root);
        match keyring::read_json("Codex", &[("service", "Codex Auth"), ("username", &account)]).await {
            Ok(found) => found,
            Err(_) if file.as_ref().is_ok_and(|doc| !doc.is_null()) => None,
            Err(error) => return Err(error),
        }
    };
    let from_keyring = keyring.is_some();
    let (doc, source) = if let Some(doc) = keyring {
        (doc, "Secret Service: Codex Auth".to_string())
    } else {
        (file?, source(&path))
    };
    Ok((doc, source, from_keyring))
}

pub(super) fn codex_account_fingerprint(document: &Value) -> String {
    let token = text(&document["tokens"]["access_token"])
        .or_else(|| text(&document["OPENAI_API_KEY"]))
        .expect("discovered Codex credentials contain a token");
    quotaline_credentials::codex_account_fingerprint(
        token,
        text(&document["tokens"]["account_id"]),
        text(&document["tokens"]["id_token"]),
    )
}

pub(super) async fn codex_remote(
    client: &Api,
    root: &Path,
    allow_token_refresh: bool,
    mut doc: Value,
    source: String,
    from_keyring: bool,
    account_selection: Option<quotaline_quota::AccountSelection>,
) -> Result<Provider, Failure> {
    if text(&doc["tokens"]["access_token"]).is_none() {
        return Err(Failure::new("sign-in", quotaline_credentials::CODEX_API_KEY_USAGE_UNAVAILABLE).with_credential());
    }
    if quotaline_credentials::credential_expired(
        text(&doc["tokens"]["access_token"]).and_then(quotaline_credentials::jwt_expiry),
        Timestamp::now().as_second() as f64,
    ) {
        if !allow_token_refresh {
            return Err(Failure::new(
                "sign-in",
                "Codex token expired. Enable token refresh or sign in with Codex CLI.",
            )
            .with_expired());
        }
        doc = refresh_codex(client, root, doc, from_keyring)
            .await
            .map_err(Failure::with_expired)?;
    }
    let token = text(&doc["tokens"]["access_token"])
        .or_else(|| text(&doc["OPENAI_API_KEY"]))
        .expect("discovered Codex credentials contain a token");
    let fetch_account = codex_fetch::Account {
        base: &client.codex_base,
        token,
        account_id: text(&doc["tokens"]["account_id"]),
    };
    let data = codex_fetch::fetch_usage(client, &fetch_account)
        .await
        .map_err(|error| pipeline_failure(error, "codex").with_credential())?;
    let account = text(&doc["tokens"]["account_id"])
        .map(str::to_owned)
        .or_else(|| credentials::jwt_claims(token).and_then(|claims| text(&claims["sub"]).map(str::to_owned)))
        .unwrap_or_else(|| token.into());
    let now = Timestamp::now().as_second() as f64;
    let usage = codex::codex_usage::usage(&data, now);
    let reported = usage.reset_credits.clone();
    let (reset_credits, warning) = codex_reset_credits(client, &fetch_account, &account, reported).await;
    let mut provider = usage.into_provider(reset_credits, source, now);
    provider.account_selection = account_selection;
    provider.account_fingerprint = Some(codex_account_fingerprint(&doc));
    confirm_codex_resets(client, root, &account, &mut provider).await;
    provider.subscription_active_until = codex_subscription_until(&doc);
    if let Some(warning) = warning {
        provider.warnings.push(warning);
    }
    Ok(provider)
}

pub(super) async fn confirm_codex_resets(client: &Api, root: &Path, account: &str, provider: &mut Provider) {
    let fingerprint: [u8; 32] = Sha256::digest(format!("{}:{account}", root.display()).as_bytes()).into();
    let mut state = client.codex_resets.lock().await;
    if state.account != Some(fingerprint) {
        *state = CodexResetState {
            account: Some(fingerprint),
            ..CodexResetState::default()
        };
    }
    provider.windows = state.guard.confirm(
        std::mem::take(&mut provider.windows),
        Timestamp::now().as_second() as f64,
    );
}

pub(super) async fn refresh_codex(
    client: &Api,
    root: &Path,
    document: Value,
    from_keyring: bool,
) -> Result<Value, Failure> {
    let path = root.join("auth.json");
    let expected = document.clone();
    let original = if from_keyring {
        None
    } else {
        Some(
            quotaline_storage::CredentialFileGuard::observe(&path, &document)
                .map_err(|error| credential_file_failure("Codex", error))?,
        )
    };
    let refresh_token = text(&document["tokens"]["refresh_token"])
        .ok_or_else(|| Failure::new("sign-in", "Codex refresh token is missing; sign in with Codex CLI."))?;
    let response = oauth_request(
        client,
        "codex",
        &document,
        client
            .http
            .post(&client.codex_token)
            .header("Accept", "application/json")
            .json(&quotaline_credentials::codex_refresh_request(refresh_token)),
    )
    .await?;
    let provider = quotaline_credentials::RefreshProvider::Codex;
    let document = quotaline_credentials::complete_refresh(
        provider,
        &document,
        &response,
        Timestamp::now().as_millisecond(),
        &Timestamp::now().display_with_offset(jiff::tz::Offset::UTC).to_string(),
    )
    .map_err(|error| Failure::new("unreadable", error.message(provider)))?;
    if let Some(original) = original {
        original
            .replace(&document)
            .map_err(|error| credential_file_failure("Codex", error))?;
    } else {
        let account = credentials::codex_key(root);
        keyring::replace(
            "Codex",
            &[("service", "Codex Auth"), ("username", &account)],
            &expected,
            &document,
        )
        .await?;
    }
    Ok(document)
}
