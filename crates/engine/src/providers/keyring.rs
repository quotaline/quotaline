//! Reads and replaces provider credentials held in the Secret Service.

use super::*;
use secret_service::{EncryptionType, Item, SearchItemsResult, SecretService};

async fn within<T>(
    operation: impl Future<Output = Result<T, secret_service::Error>>,
    timed_out: String,
    failed: String,
) -> Result<T, Failure> {
    tokio::time::timeout(Duration::from_secs(2), operation)
        .await
        .map_err(|_| Failure::new("unreadable", timed_out))?
        .map_err(|_| Failure::new("unreadable", failed))
}

pub(super) async fn connect() -> Option<SecretService<'static>> {
    // A missing or slow Secret Service means no keyring credentials, not a failure.
    tokio::time::timeout(Duration::from_millis(800), SecretService::connect(EncryptionType::Dh))
        .await
        .ok()?
        .ok()
}

pub(super) async fn search<'a>(
    product: &str,
    service: &'a SecretService<'_>,
    attributes: &[(&str, &str)],
) -> Result<SearchItemsResult<Item<'a>>, Failure> {
    within(
        service.search_items(attributes.iter().copied().collect()),
        format!("{product} keyring lookup timed out"),
        format!("Cannot search {product} keyring"),
    )
    .await
}

pub(super) async fn secret(product: &str, item: &Item<'_>) -> Result<Vec<u8>, Failure> {
    within(
        item.get_secret(),
        format!("{product} keyring read timed out"),
        format!("Cannot read {product} keyring item"),
    )
    .await
}

pub(super) async fn attributes(product: &str, item: &Item<'_>) -> Result<HashMap<String, String>, Failure> {
    within(
        item.get_attributes(),
        format!("{product} keyring account lookup timed out"),
        format!("Cannot read {product} keyring account"),
    )
    .await
}

pub(super) fn locked(product: &str) -> Failure {
    Failure::new("unreadable", format!("{product} keyring is locked"))
}

pub(super) async fn read(product: &str, attributes: &[(&str, &str)]) -> Result<Option<Vec<u8>>, Failure> {
    let Some(service) = connect().await else {
        return Ok(None);
    };
    let found = search(product, &service, attributes).await?;
    if let Some(item) = found.unlocked.first() {
        return secret(product, item).await.map(Some);
    }
    if !found.locked.is_empty() {
        return Err(locked(product));
    }
    Ok(None)
}

pub(super) async fn read_json(product: &str, attributes: &[(&str, &str)]) -> Result<Option<Value>, Failure> {
    read(product, attributes)
        .await?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|_| Failure::new("unreadable", format!("{product} keyring item is not JSON")))
        })
        .transpose()
}

pub(super) async fn replace(
    product: &str,
    attributes: &[(&str, &str)],
    expected: &Value,
    document: &Value,
) -> Result<(), Failure> {
    let service = within(
        SecretService::connect(EncryptionType::Dh),
        format!("{product} keyring connection timed out"),
        format!("Cannot connect to {product} keyring"),
    )
    .await?;
    let found = search(product, &service, attributes).await?;
    if !found.locked.is_empty() {
        return Err(Failure::new(
            "unreadable",
            format!("{product} keyring item locked during refresh"),
        ));
    }
    let changed = format!("{product} keyring item changed during refresh");
    let item = found
        .unlocked
        .first()
        .ok_or_else(|| Failure::new("sign-in", changed.clone()))?;
    let current: Value = serde_json::from_slice(&secret(product, item).await?)
        .map_err(|_| Failure::new("unreadable", changed.clone()))?;
    if &current != expected {
        return Err(Failure::new("sign-in", changed));
    }
    let serialized = serde_json::to_vec(document)
        .map_err(|_| Failure::new("unreadable", format!("Cannot encode refreshed {product} credentials")))?;
    within(
        item.set_secret(&serialized, "application/json"),
        format!("{product} keyring write timed out"),
        format!("Cannot save refreshed {product} keyring item"),
    )
    .await
}
