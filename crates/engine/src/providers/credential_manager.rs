//! Reads and replaces provider credentials held in the Windows Credential Manager.

use super::*;
use std::{ptr, slice};
use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, GetLastError},
    Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredEnumerateW, CredFree, CredReadW, CredWriteW},
};

/// How provider details name this store.
pub(super) const STORE: &str = "Credential Manager";

pub(super) struct Entry {
    pub(super) data: Vec<u8>,
    pub(super) account: Option<String>,
    target: String,
    wide: bool,
    persist: u32,
}

/// Splits the Secret Service attributes callers pass into a service and account.
fn entry<'a>(attributes: &[(&str, &'a str)]) -> (&'a str, Option<&'a str>) {
    let find = |name: &str| attributes.iter().find(|(key, _)| *key == name).map(|(_, value)| *value);
    (
        find("service").expect("credential lookups name a service"),
        find("username"),
    )
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Copies a NUL-terminated UTF-16 string the Credential Manager owns.
unsafe fn text(pointer: *const u16) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let length = (0..).take_while(|&index| unsafe { *pointer.add(index) } != 0).count();
    Some(String::from_utf16_lossy(unsafe {
        slice::from_raw_parts(pointer, length)
    }))
}

unsafe fn found(credential: &CREDENTIALW) -> Entry {
    let blob = if credential.CredentialBlob.is_null() {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(credential.CredentialBlob, credential.CredentialBlobSize as usize) }
    };
    // keyring-rs, behind Codex, Gemini CLI and Antigravity, stores text as UTF-16; keytar stores UTF-8. Their
    // secrets are JSON or tokens, so every other byte of a UTF-16 one is zero.
    let wide = blob.len() >= 2 && blob.len() % 2 == 0 && blob.iter().skip(1).step_by(2).all(|byte| *byte == 0);
    let data = if wide {
        let units: Vec<u16> = blob
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units).into_bytes()
    } else {
        blob.to_vec()
    };
    Entry {
        data,
        account: unsafe { text(credential.UserName) }.filter(|account| !account.is_empty()),
        target: unsafe { text(credential.TargetName) }.unwrap_or_default(),
        wide,
        persist: credential.Persist,
    }
}

fn read_target(target: &str) -> Result<Option<Entry>, u32> {
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    if unsafe { CredReadW(wide(target).as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(None),
            error => Err(error),
        };
    }
    let entry = unsafe { credential.as_ref() }.map(|credential| unsafe { found(credential) });
    unsafe { CredFree(credential.cast()) };
    Ok(entry)
}

fn first_matching(filter: &str) -> Result<Option<Entry>, u32> {
    let (mut count, mut credentials): (u32, *mut *mut CREDENTIALW) = (0, ptr::null_mut());
    if unsafe { CredEnumerateW(wide(filter).as_ptr(), 0, &mut count, &mut credentials) } == 0 {
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(None),
            error => Err(error),
        };
    }
    if credentials.is_null() {
        return Ok(None);
    }
    let entries = unsafe { slice::from_raw_parts(credentials, count as usize) };
    let mut entries: Vec<Entry> = entries
        .iter()
        .filter_map(|&credential| unsafe { credential.as_ref() })
        .filter(|credential| credential.Type == CRED_TYPE_GENERIC)
        .map(|credential| unsafe { found(credential) })
        .collect();
    unsafe { CredFree(credentials.cast()) };
    entries.sort_by(|left, right| left.target.cmp(&right.target));
    Ok(entries.into_iter().next())
}

fn lookup(service: &str, account: Option<&str>) -> Result<Option<Entry>, u32> {
    // keyring-rs names an item `account.service`; keytar names it `service/account`.
    match account {
        Some(account) => {
            for target in [format!("{account}.{service}"), format!("{service}/{account}")] {
                if let Some(entry) = read_target(&target)? {
                    return Ok(Some(entry));
                }
            }
            Ok(None)
        }
        None => {
            for filter in [format!("*.{service}"), format!("{service}/*")] {
                if let Some(entry) = first_matching(&filter)? {
                    return Ok(Some(entry));
                }
            }
            Ok(None)
        }
    }
}

/// Reads the item for `account`, or the first item of `service` when no account is named.
pub(super) async fn load(product: &str, service: &str, account: Option<&str>) -> Result<Option<Entry>, Failure> {
    lookup(service, account).map_err(|error| {
        Failure::new(
            "unreadable",
            format!("Cannot read {product} Credential Manager item (error {error})"),
        )
    })
}

pub(super) async fn read(product: &str, attributes: &[(&str, &str)]) -> Result<Option<Vec<u8>>, Failure> {
    let (service, account) = entry(attributes);
    Ok(load(product, service, account).await?.map(|entry| entry.data))
}

pub(super) async fn read_json(product: &str, attributes: &[(&str, &str)]) -> Result<Option<Value>, Failure> {
    read(product, attributes)
        .await?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|_| Failure::new("unreadable", format!("{product} Credential Manager item is not JSON")))
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
    let changed = format!("{product} Credential Manager item changed during refresh");
    let current = load(product, service, account)
        .await?
        .ok_or_else(|| Failure::new("sign-in", changed.clone()))?;
    let stored: Value =
        serde_json::from_slice(&current.data).map_err(|_| Failure::new("unreadable", changed.clone()))?;
    if &stored != expected {
        return Err(Failure::new("sign-in", changed));
    }
    let serialized = serde_json::to_string(document)
        .map_err(|_| Failure::new("unreadable", format!("Cannot encode refreshed {product} credentials")))?;
    let mut blob = if current.wide {
        serialized.encode_utf16().flat_map(u16::to_le_bytes).collect()
    } else {
        serialized.into_bytes()
    };
    let mut target = wide(&current.target);
    let mut user = current.account.as_deref().map(wide);
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: current.persist,
        UserName: user.as_mut().map_or(ptr::null_mut(), |user| user.as_mut_ptr()),
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err(Failure::new(
            "unreadable",
            format!(
                "Cannot save refreshed {product} Credential Manager item (error {})",
                unsafe { GetLastError() }
            ),
        ));
    }
    Ok(())
}
