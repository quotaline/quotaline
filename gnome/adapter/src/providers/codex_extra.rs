//! Collects Codex daily analytics and credit events.

use super::*;

pub(super) async fn codex_reset_credits(
    client: &Api,
    account: &codex_fetch::Account<'_>,
    key: &str,
    reported: Option<ResetCredits>,
) -> (Option<ResetCredits>, Option<String>) {
    let complete = reported.as_ref().is_some_and(ResetCredits::complete);
    let cache = &client.codex_reset_cache;
    let cached = cache
        .lock()
        .await
        .read(key, reported, complete, client.cache_epoch.elapsed().as_secs_f64());
    if !cached.should_fetch {
        return (cached.value, cached.warning);
    }
    let (value, warning) = match codex_fetch::fetch_reset_credits(client, account).await {
        Ok(credits) => (Some(credits), None),
        Err(error) => (
            cached.value,
            Some(codex_fetch::reset_credits_warning(
                &error,
                env::var("HOME").ok().as_deref(),
            )),
        ),
    };
    cache.lock().await.record(
        key,
        value.clone(),
        warning.clone(),
        client.cache_epoch.elapsed().as_secs_f64(),
    );
    (value, warning)
}

pub(super) fn codex_provider(data: &Value, source: String) -> Provider {
    let now = Timestamp::now().as_second() as f64;
    codex::codex_usage::usage(data, now).into_provider(None, source, now)
}

pub struct CodexAnalytics {
    pub snapshot: Option<AnalyticsSnapshot>,
    pub watermarks: Option<Vec<u8>>,
}

/// Fetches Codex analytics from the stored watermarks; the caller saves the new watermarks once the snapshot is stored.
pub async fn codex_analytics(
    client: &Api,
    home: &Path,
    retention_days: u32,
    stored: Option<&[u8]>,
) -> Option<CodexAnalytics> {
    let configured = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let (_, doc, _, _, _) = discover_codex_document(client, &configured, home).await.ok()?;
    let token = text(&doc["tokens"]["access_token"])
        .or_else(|| text(&doc["OPENAI_API_KEY"]))
        .expect("discovered Codex credentials contain a token");
    let fingerprint = codex_account_fingerprint(&doc);
    let now = Timestamp::now().as_millisecond() as f64 / 1000.0;
    let watermarks = std::sync::Mutex::new(Watermarks::load(stored, fingerprint.clone(), now, retention_days));
    let account = codex_fetch::Account {
        base: &client.codex_base,
        token,
        account_id: text(&doc["tokens"]["account_id"]),
    };
    let home = home.to_string_lossy();
    let context = codex_fetch::AnalyticsContext {
        fingerprint: &fingerprint,
        now,
        retention_days,
        home: Some(&home),
    };
    let result = codex_fetch::fetch_analytics(client, &account, &watermarks, &context).await;
    for warning in &result.warnings {
        eprintln!("Codex analytics: {warning}");
    }
    Some(CodexAnalytics {
        snapshot: result.snapshot,
        watermarks: watermarks
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .encoded_if_changed(),
    })
}
