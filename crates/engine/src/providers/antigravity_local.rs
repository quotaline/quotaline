//! Discovers Antigravity local language servers.

use procfs::FromReadSI;
use procfs::net::{TcpNetEntries, TcpState};
use procfs::process::{FDTarget, all_processes_with_root};
use quotaline_antigravity::local::LocalRole;
use quotaline_antigravity::local::role;
use quotaline_antigravity::validate;
use reqwest::Client;
use serde_json::Value;
use std::{collections::HashMap, env, ffi::OsStr, fs, os::unix::fs::MetadataExt, path::PathBuf, time::Duration};

struct Server {
    ports: Vec<u16>,
    csrf: Option<String>,
    source: &'static str,
}

fn discover() -> Vec<Server> {
    #[cfg(feature = "fixture-api")]
    let root = env::var_os("TMB_FIXTURE_PROC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/proc"));
    #[cfg(not(feature = "fixture-api"))]
    let root = PathBuf::from("/proc");
    let sockets: HashMap<u64, u16> = ["net/tcp", "net/tcp6"]
        .into_iter()
        .filter_map(|table| TcpNetEntries::from_file(root.join(table), procfs::current_system_info()).ok())
        .flat_map(|entries| entries.0)
        .filter(|entry| entry.state == TcpState::Listen)
        .map(|entry| (entry.inode, entry.local_address.port()))
        .collect();
    let uid = fs::metadata(root.join("self"))
        .map(|item| item.uid())
        .unwrap_or(u32::MAX);
    let runtime = env::var_os("XDG_RUNTIME_DIR");
    let Ok(processes) = all_processes_with_root(&root) else {
        return Vec::new();
    };
    processes
        .flatten()
        .filter(|process| process.uid().is_ok_and(|owner| owner == uid))
        .filter_map(|process| {
            let args = process.cmdline().ok()?;
            let args: Vec<_> = args.iter().map(String::as_str).collect();
            let (csrf, source) = match role(args.first()?, &args)? {
                LocalRole::Cli => (None, "Antigravity CLI"),
                LocalRole::LanguageServer { csrf_token } => (Some(csrf_token), "Antigravity language server"),
            };
            if let Some(runtime) = &runtime
                && process.environ().ok()?.get(OsStr::new("XDG_RUNTIME_DIR")) != Some(runtime)
            {
                return None;
            }
            let mut ports: Vec<_> = process
                .fd()
                .ok()?
                .flatten()
                .filter_map(|fd| match fd.target {
                    FDTarget::Socket(inode) => sockets.get(&inode).copied(),
                    _ => None,
                })
                .collect();
            ports.sort_unstable();
            ports.dedup();
            (!ports.is_empty()).then_some(Server { ports, csrf, source })
        })
        .collect()
}

async fn post(client: &Client, url: String, csrf: Option<&str>, body: Value) -> Option<Value> {
    let mut request = client.post(url).json(&body);
    for (name, value) in quotaline_antigravity::request::server_headers(csrf) {
        request = request.header(name, value);
    }
    request.send().await.ok()?.error_for_status().ok()?.json().await.ok()
}

pub async fn quota() -> Option<(Value, Value, &'static str)> {
    let servers = discover();
    if servers.is_empty() {
        return None;
    }
    // The language server binds loopback. TLS certificates are self-signed, so this client
    // is used only for an address discovered from a process owned by this user.
    let client = Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(u64::from(
            quotaline_antigravity::request::SERVER_TIMEOUT_MILLIS,
        )))
        .build()
        .ok()?;
    for server in servers {
        for port in server.ports {
            for scheme in quotaline_antigravity::request::SERVER_SCHEMES {
                let Some(summary) = post(
                    &client,
                    quotaline_antigravity::request::server_quota_url(scheme, port),
                    server.csrf.as_deref(),
                    quotaline_antigravity::request::server_quota_body(),
                )
                .await
                else {
                    continue;
                };
                if validate(&summary).is_err() {
                    continue;
                }
                let status = post(
                    &client,
                    quotaline_antigravity::request::server_status_url(scheme, port),
                    server.csrf.as_deref(),
                    quotaline_antigravity::request::server_status_body(),
                )
                .await
                .unwrap_or(Value::Null);
                return Some((summary, status, server.source));
            }
        }
    }
    None
}
