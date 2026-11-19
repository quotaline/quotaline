//! Runs the GNOME D-Bus service that serves the engine's state to GNOME Shell.

use futures_util::{StreamExt, TryStreamExt, future::BoxFuture};
use jiff::Timestamp;
use quotaline_engine::{
    clock::SessionClock,
    service::{Engine, Host},
    store,
};
use quotaline_export as export;
use quotaline_notifications as notifications;
use std::{
    collections::BTreeMap,
    env,
    path::PathBuf,
    process::Command,
    sync::{Arc, OnceLock},
};
use tokio::sync::Notify;
use zbus::{connection::Builder, object_server::SignalEmitter};

const NAME: &str = "tech.bernat.Quotaline.Gnome";
const PATH: &str = "/tech/bernat/Quotaline/Gnome";

#[zbus::proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait PortalSettings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<zbus::OwnedVariant>;
}

/// Turns engine changes into the service's D-Bus signals.
#[derive(Default)]
struct GnomeHost {
    // Set after the engine is built, so a broken fixture configuration fails before the session bus is needed.
    connection: OnceLock<zbus::Connection>,
}

impl GnomeHost {
    fn connection(&self) -> &zbus::Connection {
        self.connection
            .get()
            .expect("the engine runs only after the service connects")
    }
}

impl Host for GnomeHost {
    fn changed(&self, state: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _: zbus::Result<()> = Bus::changed(&emitter(self.connection()), &state).await;
        })
    }

    fn log_changed(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _: zbus::Result<()> = Bus::log_changed(&emitter(self.connection())).await;
        })
    }

    fn notification_planned(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _: zbus::Result<()> = Bus::notification_planned(&emitter(self.connection())).await;
        })
    }

    fn twelve_hour_clock(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move { twelve_hour_clock(self.connection()).await })
    }
}

async fn twelve_hour_clock(connection: &zbus::Connection) -> bool {
    let format = async {
        PortalSettingsProxy::new(connection)
            .await?
            .read_one("org.gnome.desktop.interface", "clock-format")
            .await
    }
    .await;
    format.ok().and_then(|value| String::try_from(value).ok()).as_deref() == Some("12h")
}

async fn deliver_notifications(engine: &Engine, connection: &zbus::Connection) {
    let planned = engine.take_notifications().await;
    if planned.is_empty() {
        return;
    }
    let clock = SessionClock::new(jiff::tz::TimeZone::system(), twelve_hour_clock(connection).await);
    let now = Timestamp::now().as_second();
    for item in planned {
        // GNOME plans sign-in notices with the provider's own message, so no login hint is needed here.
        let copy = notifications::copy(
            &item.event.facts,
            &item.provider,
            "",
            notifications::Moments {
                now,
                fetched_at: item.fetched_at,
            },
            &clock,
        );
        notify(engine, connection, &copy.title, &copy.body, item.sign_in.as_deref()).await;
        #[cfg(feature = "fixture-api")]
        engine.record_notification_handoff().await;
    }
}

async fn notify(engine: &Engine, connection: &zbus::Connection, title: &str, body: &str, provider: Option<&str>) {
    let (enabled, sound) = engine.notification_preferences().await;
    if !enabled {
        return;
    }
    let mut hints = BTreeMap::<String, zbus::Variant<'_>>::new();
    if sound {
        hints.insert("sound-name".into(), zbus::Variant::from("message-new-instant"));
    } else {
        hints.insert("suppress-sound".into(), zbus::Variant::from(true));
    }
    let actions = if provider.is_some() {
        vec!["default", "Sign in"]
    } else {
        Vec::new()
    };
    let result: zbus::Result<u32> = async {
        let reply = connection
            .call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &(
                    "Quotaline",
                    0_u32,
                    "dialog-information-symbolic",
                    title,
                    body,
                    actions,
                    hints,
                    7000_i32,
                ),
            )
            .await?;
        reply.body().deserialize()
    }
    .await;
    if let (Ok(notification_id), Some(provider)) = (result, provider) {
        engine.route_notification(notification_id, provider).await;
    }
}

async fn watch_notification_actions(engine: &Engine, connection: &zbus::Connection) -> zbus::Result<()> {
    #[cfg(feature = "fixture-api")]
    let interface =
        env::var("TMB_FIXTURE_NOTIFICATION_INTERFACE").unwrap_or_else(|_| "org.freedesktop.Notifications".into());
    #[cfg(not(feature = "fixture-api"))]
    let interface = String::from("org.freedesktop.Notifications");
    let proxy = zbus::Proxy::new(
        connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        interface.as_str(),
    )
    .await?;
    let mut actions = proxy.receive_signal("ActionInvoked").await?;
    while let Some(message) = tokio::select! {
        message = actions.next() => message,
        _ = connection.closed() => None,
    } {
        let Ok((notification_id, action)) = message.body().deserialize::<(u32, String)>() else {
            continue;
        };
        if action != "default" {
            continue;
        }
        if let Some(provider) = engine.notification_sign_in(notification_id).await {
            let _ = Bus::sign_in_requested(&emitter(connection), &provider).await;
        }
    }
    Ok(())
}

async fn watch_power_profiles(engine: &Engine, stop: &Notify) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.UPower.PowerProfiles",
        "/org/freedesktop/UPower/PowerProfiles",
        "org.freedesktop.UPower.PowerProfiles",
    )
    .await?;
    let active = proxy.receive_property_changed::<String>("ActiveProfile").await;
    let degraded = proxy.receive_property_changed::<String>("PerformanceDegraded").await;
    futures_util::stream::select(
        active.map(|change| Ok::<_, zbus::Error>((true, change))),
        degraded.map(|change| Ok::<_, zbus::Error>((false, change))),
    )
    .take_until(stop.notified())
    .try_for_each(|(is_profile, change)| async move {
        let value = change.get().await.ok();
        if is_profile {
            engine
                .set_power_state(value.map(|value| value == "power-saver"), None)
                .await;
        } else {
            engine
                .set_power_state(None, value.map(|value| value == "high-operating-temperature"))
                .await;
        }
        Ok(())
    })
    .await
}

async fn start_sign_in(engine: &Engine, provider: &str) -> bool {
    if engine.is_demo().await {
        return false;
    }
    let command: &[&str] = match provider {
        "claude" => &["claude", "auth", "login"],
        "codex" => &["codex", "login"],
        "gemini" => &["gemini"],
        "cursor" => &["cursor-agent", "login"],
        "copilot" => &["copilot", "login"],
        _ => return false,
    };
    let mut search = env::var_os("PATH").unwrap_or_default();
    search.push(":");
    search.push(engine.home().join(".local/bin"));
    let find = |name: &str| -> Option<PathBuf> {
        env::split_paths(&search)
            .map(|directory| directory.join(name))
            .find(|path| path.is_file())
    };
    let Some(executable) = find(command[0]) else {
        return false;
    };
    for terminal in ["ptyxis", "kgx", "gnome-terminal"] {
        let Some(program) = find(terminal) else {
            continue;
        };
        let mut process = Command::new(program);
        if terminal == "ptyxis" {
            process.arg("--new-window").arg("--");
        } else {
            process.arg("--");
        }
        match process
            .arg(&executable)
            .args(&command[1..])
            .env("PATH", &search)
            .spawn()
        {
            Ok(_) => return true,
            Err(error) => {
                engine
                    .log("warning", "app", &format!("{provider} sign-in launch failed: {error}"))
                    .await
            }
        }
    }
    false
}

fn emitter(connection: &zbus::Connection) -> SignalEmitter<'static> {
    SignalEmitter::new(connection, PATH).expect("service path is valid")
}

mod bus;

use bus::Bus;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let demo_requested = match args.as_slice() {
        [] => env::var_os("QUOTALINE_DEMO").is_some(),
        [flag] if flag == "--demo" => true,
        [flag] if flag == "--usage-json" => {
            let snapshot = store::read_snapshot();
            let export = export::UsageExport::from(&snapshot);
            if export.is_empty() {
                eprintln!("usage export failed: no cached usage yet");
                std::process::exit(1);
            }
            println!("{}", serde_json::to_string_pretty(&export)?);
            return Ok(());
        }
        [flag] if flag == "--help" => {
            println!("Usage: quotaline-gnome [--demo | --usage-json | --help]");
            return Ok(());
        }
        _ => {
            eprintln!("Unsupported argument. Run quotaline-gnome --help for usage.");
            std::process::exit(2);
        }
    };
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let host = Arc::new(GnomeHost::default());
    let engine = Engine::new(demo_requested, host.clone())?;
    let connection = Builder::session().build().await?;
    host.connection
        .set(connection.clone())
        .expect("the service connects once");
    connection.object_server().at(PATH, Bus(engine.clone())).await?;
    connection.request_name(NAME).await?;
    let notification_engine = engine.clone();
    let notification_connection = connection.clone();
    let notification_listener = tokio::spawn(async move {
        if let Err(error) = watch_notification_actions(&notification_engine, &notification_connection).await {
            notification_engine
                .log("warning", "app", &format!("Notification actions unavailable: {error}"))
                .await;
        }
    });
    let power_stop = Arc::new(Notify::new());
    let power_engine = engine.clone();
    let power_wait = power_stop.clone();
    let power_listener = tokio::spawn(async move {
        let _ = watch_power_profiles(&power_engine, &power_wait).await;
    });
    tokio::select! {
        () = engine.run() => {},
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {},
        _ = connection.closed() => {},
    }
    engine.release().await;
    power_stop.notify_one();
    if !connection.is_closed() {
        connection.close().await?;
    }
    let _ = power_listener.await;
    let _ = notification_listener.await;
    Ok(())
}
