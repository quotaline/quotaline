//! Reads and replaces provider credentials held in the macOS Keychain.

use super::*;
use quotaline_macos_platform::keychain::{
    is_denial,
    native::{Item, Store},
};

/// How provider details name this store.
pub(super) const STORE: &str = "Keychain";

/// Splits the Secret Service attributes callers pass into a Keychain service and account.
fn entry<'a>(attributes: &[(&str, &'a str)]) -> (&'a str, Option<&'a str>) {
    let find = |name: &str| attributes.iter().find(|(key, _)| *key == name).map(|(_, value)| *value);
    (
        find("service").expect("credential lookups name a service"),
        find("username"),
    )
}

fn failure(product: &str, status: i32) -> Failure {
    Failure::new(
        "unreadable",
        if is_denial(status) {
            format!("{product} Keychain access was denied")
        } else {
            format!("Cannot read {product} Keychain item (status {status})")
        },
    )
}

/// Reads the item for `account`, or the first item of `service` when no account is named.
pub(super) async fn load(product: &str, service: &str, account: Option<&str>) -> Result<Option<Item>, Failure> {
    let (service, account) = (service.to_owned(), account.map(str::to_owned));
    // Reading another app's item can open a macOS permission prompt, which blocks until the user answers.
    tokio::task::spawn_blocking(move || Store::system().load(&service, account.as_deref()))
        .await
        .map_err(|_| Failure::new("unreadable", format!("{product} Keychain read stopped")))?
        .map_err(|status| failure(product, status))
}

pub(super) async fn read(product: &str, attributes: &[(&str, &str)]) -> Result<Option<Vec<u8>>, Failure> {
    let (service, account) = entry(attributes);
    Ok(load(product, service, account).await?.map(|item| item.data))
}

pub(super) async fn read_json(product: &str, attributes: &[(&str, &str)]) -> Result<Option<Value>, Failure> {
    read(product, attributes)
        .await?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|_| Failure::new("unreadable", format!("{product} Keychain item is not JSON")))
        })
        .transpose()
}

pub(super) async fn replace(
    product: &str,
    attributes: &[(&str, &str)],
    expected: &Value,
    document: &Value,
) -> Result<(), Failure> {
    let (service, account) = entry(attributes);
    let changed = format!("{product} Keychain item changed during refresh");
    let item = load(product, service, account)
        .await?
        .ok_or_else(|| Failure::new("sign-in", changed.clone()))?;
    let current: Value = serde_json::from_slice(&item.data).map_err(|_| Failure::new("unreadable", changed.clone()))?;
    if &current != expected {
        return Err(Failure::new("sign-in", changed));
    }
    let serialized = serde_json::to_vec(document)
        .map_err(|_| Failure::new("unreadable", format!("Cannot encode refreshed {product} credentials")))?;
    let (service, account) = (
        service.to_owned(),
        item.account.or_else(|| account.map(str::to_owned)).unwrap_or_default(),
    );
    tokio::task::spawn_blocking(move || Store::system().save(&serialized, &service, &account))
        .await
        .map_err(|_| Failure::new("unreadable", format!("{product} Keychain write stopped")))?
        .map_err(|_| Failure::new("unreadable", format!("Cannot save refreshed {product} Keychain item")))
}
