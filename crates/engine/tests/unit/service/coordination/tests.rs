use super::*;
use rusqlite::Connection;
use serde_json::Value;
use std::sync::atomic::AtomicUsize;
use tokio::sync::watch;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

struct Recorder {
    state: watch::Sender<Value>,
    planned: AtomicUsize,
}

impl Host for Recorder {
    fn changed(&self, state: String) -> BoxFuture<'_, ()> {
        self.state.send_replace(serde_json::from_str(&state).unwrap());
        Box::pin(async {})
    }

    fn log_changed(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn notification_planned(&self) -> BoxFuture<'_, ()> {
        self.planned.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {})
    }

    fn twelve_hour_clock(&self) -> BoxFuture<'_, bool> {
        Box::pin(async { false })
    }
}

struct Running {
    engine: Engine,
    host: Arc<Recorder>,
    run: JoinHandle<()>,
}

fn recorder() -> Arc<Recorder> {
    Arc::new(Recorder {
        state: watch::channel(Value::Null).0,
        planned: AtomicUsize::new(0),
    })
}

impl Running {
    fn start() -> Self {
        Self::launch(false)
    }

    fn launch(demo: bool) -> Self {
        let host = recorder();
        let engine = Engine::new(demo, host.clone()).unwrap();
        let run = tokio::spawn({
            let engine = engine.clone();
            async move { engine.run().await }
        });
        Self { engine, host, run }
    }

    async fn used(&self, used: f64) {
        let mut states = self.host.state.subscribe();
        tokio::time::timeout(
            Duration::from_secs(30),
            states.wait_for(|state| state["providers"]["claude"]["windows"][0]["used"] == used),
        )
        .await
        .unwrap()
        .unwrap();
    }
}

const RATE_LIMITED: u32 = u32::MAX;

/// Points every engine in this test process at one machine, with Claude the only provider and its usage served by
/// the mock, which reports each fetch's utilization from `readings`, or a rate limit for `RATE_LIMITED`, and repeats
/// the last one.
async fn machine(readings: &'static [u32]) -> (tempfile::TempDir, MockServer, Arc<AtomicUsize>) {
    let root = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let fetches = Arc::new(AtomicUsize::new(0));
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with({
            let fetches = fetches.clone();
            move |_: &wiremock::Request| {
                let fetch = fetches.fetch_add(1, Ordering::SeqCst);
                match readings[fetch.min(readings.len() - 1)] {
                    RATE_LIMITED => ResponseTemplate::new(429).insert_header("retry-after", "600"),
                    utilization => {
                        ResponseTemplate::new(200).set_body_json(json!({"five_hour": {"utilization": utilization}}))
                    }
                }
            }
        })
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    // Each nextest test runs in its own process, so no other test reads these variables meanwhile.
    unsafe {
        env::set_var("HOME", root.path().join("home"));
        env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
        env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
        env::set_var("XDG_DATA_HOME", root.path().join("data"));
        env::set_var("TMB_FIXTURE_API_URL", server.uri());
        env::remove_var("CLAUDE_CONFIG_DIR");
    }
    let credentials = root.path().join("home/.claude/.credentials.json");
    fs::create_dir_all(credentials.parent().unwrap()).unwrap();
    fs::write(
        credentials,
        r#"{"claudeAiOauth":{"accessToken":"active","expiresAt":9999999999999}}"#,
    )
    .unwrap();
    let providers = json!({"claude": true, "codex": false, "gemini": false, "antigravity": false, "cursor": false, "copilot": false});
    fs::create_dir_all(store::config().parent().unwrap()).unwrap();
    fs::write(store::config(), json!({"providers": providers}).to_string()).unwrap();
    (root, server, fetches)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn engines_share_one_poller_its_readings_and_its_notices() {
    let (_root, _server, fetches) = machine(&[42, 95]).await;
    let leader = Running::start();
    leader.used(42.0).await;
    let follower = Running::start();
    follower.used(42.0).await;
    assert_eq!(fetches.load(Ordering::SeqCst), 1, "the follower fetched on its own");

    assert!(follower.engine.refresh("claude").await);
    leader.used(95.0).await;
    follower.used(95.0).await;
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
    assert_eq!(leader.engine.pending_notification_count().await, 1);
    let notified = follower.engine.take_notifications().await;
    assert_eq!(
        notified.iter().map(|planned| planned.event.kind).collect::<Vec<_>>(),
        [notifications::Kind::Threshold]
    );
    assert!(
        leader.engine.take_notifications().await.is_empty(),
        "a notice was claimed twice"
    );
    assert!(follower.host.planned.load(Ordering::Relaxed) > 0);

    leader.engine.quit();
    leader.run.await.unwrap();
    assert!(follower.engine.refresh("claude").await);
    tokio::time::timeout(Duration::from_secs(30), async {
        while fetches.load(Ordering::SeqCst) < 3 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the follower did not take over polling");
    follower.engine.quit();
    follower.run.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn settings_written_by_one_engine_reach_the_other() {
    let (_root, _server, _fetches) = machine(&[42]).await;
    let first = Running::start();
    first.used(42.0).await;
    let second = Running::start();
    second.used(42.0).await;
    assert_eq!(second.engine.notification_preferences().await, (true, true));
    first.engine.set_setting("notify_sound", "false").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while second.engine.notification_preferences().await != (true, false) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the other engine kept the old setting");
    let coordination = second.engine.coordination.as_ref().unwrap();
    first.engine.set_setting("demo", "true").await.unwrap();
    second.engine.coordinate(coordination, false).await;
    assert!(!second.engine.is_demo().await, "demo mode spread to another engine");
    fs::remove_file(store::config()).unwrap();
    second.engine.coordinate(coordination, false).await;
    assert_eq!(second.engine.notification_preferences().await, (true, false));
}

fn leading(engine: &Engine) -> bool {
    engine.coordination.as_ref().unwrap().leading.load(Ordering::Relaxed)
}

fn rival() -> Live {
    fs::create_dir_all(store::live().parent().unwrap()).unwrap();
    Live::open(&store::live(), "rival".into()).unwrap()
}

fn published(running: &[&str]) -> Published {
    Published {
        providers: BTreeMap::new(),
        statuses: BTreeMap::new(),
        session: refresh::Handover::default(),
        running: running.iter().map(|id| (*id).to_owned()).collect(),
        last_refresh: None,
        claude_local_usage: None,
        recent_samples: BTreeMap::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_engine_follows_while_another_holds_the_lease_and_polls_once_it_is_free() {
    let (_root, _server, fetches) = machine(&[42]).await;
    let rival = rival();
    assert!(rival.lead(epoch(), 3_600.0).unwrap());
    rival.publish(&published(&["claude"])).unwrap();
    let engine = Running::start();
    let coordination = engine.engine.coordination.as_ref().unwrap();
    let mut states = engine.host.state.subscribe();
    tokio::time::timeout(
        Duration::from_secs(10),
        states.wait_for(|state| state["refreshing_providers"] == json!(["claude"])),
    )
    .await
    .expect("the follower did not show the published fetch")
    .unwrap();
    assert!(!leading(&engine.engine));
    assert!(engine.engine.refresh("claude").await);
    assert_eq!(fetches.load(Ordering::SeqCst), 0);

    // A holder that stopped without letting go: its request waits until this engine takes over.
    rival.resign().unwrap();
    engine.engine.coordinate(coordination, true).await;
    assert!(leading(&engine.engine));
    engine.used(42.0).await;

    let raw = Connection::open(store::live()).unwrap();
    raw.execute("UPDATE lease SET holder = 'rival', expires = ?1", [epoch() + 3_600.0])
        .unwrap();
    engine.engine.coordinate(coordination, true).await;
    assert!(!leading(&engine.engine));
    rival.publish(&published(&[])).unwrap();
    engine.engine.coordinate(coordination, false).await;
    assert_eq!(engine.host.state.borrow()["providers"]["claude"]["windows"], json!([]));
    engine.engine.quit();
    engine.run.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_promoted_engine_drops_fetches_the_previous_poller_left_running() {
    let (_root, _server, _fetches) = machine(&[42]).await;
    let rival = rival();
    assert!(rival.lead(epoch(), 3_600.0).unwrap());
    let engine = Engine::new(false, recorder()).unwrap();
    assert!(!engine.contest().await);
    engine.follow().await;
    assert!(
        engine.inner.lock().await.running.is_empty(),
        "nothing was published yet"
    );
    rival.publish(&published(&["codex"])).unwrap();
    engine.follow().await;
    assert!(engine.inner.lock().await.running.contains("codex"));

    // A cycle of this engine's own still runs, so its fetches stay shown until they finish.
    engine.inner.lock().await.cycle_running = true;
    rival.resign().unwrap();
    assert!(engine.contest().await);
    assert!(engine.inner.lock().await.running.contains("codex"));

    engine.inner.lock().await.cycle_running = false;
    let raw = Connection::open(store::live()).unwrap();
    raw.execute("UPDATE lease SET holder = 'rival', expires = ?1", [epoch() + 3_600.0])
        .unwrap();
    assert!(!engine.contest().await);
    rival.resign().unwrap();
    assert!(engine.contest().await);
    assert!(engine.inner.lock().await.running.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_poller_serves_forwarded_resets_and_lets_go_on_release() {
    let (_root, _server, _fetches) = machine(&[42]).await;
    let leader = Running::start();
    leader.used(42.0).await;
    let coordination = leader.engine.coordination.as_ref().unwrap();
    assert!(!leader.engine.inner.lock().await.recent_samples.is_empty());
    let rival = rival();
    let checkpoint = store::xdg("XDG_DATA_HOME", ".local/share").join("claude-transcripts.json");
    let _ = fs::remove_file(&checkpoint);
    fs::create_dir_all(checkpoint.join("blocked")).unwrap();
    rival.request(&Request::ClearHistory).unwrap();
    leader.engine.coordinate(coordination, false).await;
    assert!(
        !leader.engine.inner.lock().await.recent_samples.is_empty(),
        "a failed clear dropped readings"
    );
    fs::remove_dir_all(&checkpoint).unwrap();
    rival.request(&Request::ClearHistory).unwrap();
    rival.request(&Request::ClearHistory).unwrap();
    leader.engine.coordinate(coordination, false).await;
    assert!(leader.engine.inner.lock().await.recent_samples.is_empty());
    rival.request(&Request::ForgetReadings).unwrap();
    leader.engine.coordinate(coordination, false).await;
    assert!(leader.engine.inner.lock().await.providers.is_empty());

    leader.engine.release().await;
    assert!(!leader.engine.contest().await);
    assert!(leader.engine.refresh("claude").await);
    assert!(
        rival.lead(epoch(), 30.0).unwrap(),
        "a released engine took the lease back"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_poller_keeps_the_visible_pace_while_any_panel_is_open() {
    let (_root, _server, _fetches) = machine(&[42]).await;
    let leader = Running::start();
    leader.used(42.0).await;
    let follower = Running::start();
    follower.used(42.0).await;
    follower.engine.set_panel_open(true).await;
    follower
        .engine
        .coordinate(follower.engine.coordination.as_ref().unwrap(), true)
        .await;
    leader
        .engine
        .coordinate(leader.engine.coordination.as_ref().unwrap(), true)
        .await;
    assert!(leader.engine.inner.lock().await.remote_visible);
    follower.engine.set_panel_open(false).await;
    leader
        .engine
        .coordinate(leader.engine.coordination.as_ref().unwrap(), true)
        .await;
    assert!(!leader.engine.inner.lock().await.remote_visible);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn demo_readings_stay_with_the_engine_showing_them() {
    let (_root, _server, fetches) = machine(&[42]).await;
    let engine = Running::start();
    engine.used(42.0).await;
    engine.engine.set_setting("demo", "true").await.unwrap();
    let coordination = engine.engine.coordination.as_ref().unwrap();
    engine.engine.coordinate(coordination, false).await;
    engine.engine.coordinate(coordination, true).await;
    assert!(!leading(&engine.engine));
    engine.engine.set_panel_open(true).await;
    engine.engine.follow().await;
    assert!(!engine.engine.post_notifications(&[]).await);
    assert!(engine.engine.take_notifications().await.is_empty());
    assert_eq!(engine.engine.pending_notification_count().await, 0);
    assert!(
        !rival().visible(epoch(), 30.0).unwrap(),
        "a demo panel reached the other engines"
    );
    assert!(rival().lead(epoch(), 30.0).unwrap(), "a demo engine kept the lease");
    rival().resign().unwrap();
    engine.engine.set_setting("demo", "false").await.unwrap();
    assert!(leading(&engine.engine));
    tokio::time::timeout(Duration::from_secs(30), async {
        while fetches.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("leaving demo did not poll again");

    let requested = Running::launch(true);
    assert!(requested.engine.coordination.is_none());
    assert!(requested.engine.contest().await);
    requested.engine.quit();
    requested.run.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unusable_store_leaves_the_engine_polling_alone() {
    let (_root, _server, fetches) = machine(&[42, 95]).await;
    let engine = Running::start();
    engine.used(42.0).await;
    Connection::open(store::live())
        .unwrap()
        .execute_batch("DROP TABLE lease; DROP TABLE requests; DROP TABLE outbox;")
        .unwrap();
    engine
        .engine
        .coordinate(engine.engine.coordination.as_ref().unwrap(), true)
        .await;
    assert!(leading(&engine.engine));
    assert!(engine.engine.refresh("claude").await);
    engine.used(95.0).await;
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
    assert_eq!(
        engine.engine.take_notifications().await.len(),
        1,
        "a notice the outbox refused was lost"
    );

    fs::remove_file(store::live()).unwrap();
    fs::create_dir(store::live()).unwrap();
    assert!(Engine::new(false, recorder()).unwrap().coordination.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rate_limit_and_its_backoff_reach_the_other_engines() {
    let (_root, _server, fetches) = machine(&[42, RATE_LIMITED]).await;
    let leader = Running::start();
    leader.used(42.0).await;
    let follower = Running::start();
    follower.used(42.0).await;
    assert!(follower.engine.refresh("claude").await);
    let mut states = follower.host.state.subscribe();
    let state = tokio::time::timeout(
        Duration::from_secs(30),
        states.wait_for(|state| state["providers"]["claude"]["retry_after"].as_u64() > Some(0)),
    )
    .await
    .unwrap()
    .unwrap()
    .clone();
    assert_eq!(
        state["providers"]["claude"],
        leader.host.state.borrow()["providers"]["claude"]
    );
    let backoff = follower.engine.inner.lock().await.session.times("claude");
    assert!(backoff.rate_limited, "the backoff stayed with the engine that polled");

    leader.engine.quit();
    leader.run.await.unwrap();
    assert!(follower.engine.refresh("claude").await);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(fetches.load(Ordering::SeqCst), 2, "taking over ignored the rate limit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_provider_turned_off_is_not_fetched_even_when_named() {
    let (_root, _server, fetches) = machine(&[42]).await;
    // Not running, so no poller takes the signal a cycle gives when it ends.
    let engine = Engine::new(false, recorder()).unwrap();
    assert!(engine.contest().await);
    let off = json!({"claude": false, "codex": false, "gemini": false, "antigravity": false, "cursor": false, "copilot": false});
    engine.apply_setting("providers", &off.to_string()).await.unwrap();
    engine.schedule_changed.notified().await;

    assert!(engine.refresh("claude").await);
    tokio::time::timeout(Duration::from_secs(30), engine.schedule_changed.notified())
        .await
        .unwrap();
    assert_eq!(fetches.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turning_a_provider_on_fetches_it_once() {
    let (_root, _server, fetches) = machine(&[42]).await;
    let off = json!({"claude": false, "codex": false, "gemini": false, "antigravity": false, "cursor": false, "copilot": false});
    fs::write(store::config(), json!({"providers": off}).to_string()).unwrap();
    let running = Running::start();
    let mut on = off.clone();
    on["claude"] = true.into();
    running
        .engine
        .apply_setting("providers", &on.to_string())
        .await
        .unwrap();
    running.used(42.0).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    running.engine.quit();
    running.run.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refresh_of_a_provider_being_fetched_waits_for_that_fetch() {
    let (_root, _server, fetches) = machine(&[42]).await;
    let engine = Engine::new(false, recorder()).unwrap();
    assert!(engine.contest().await);
    engine.inner.lock().await.running.insert("claude".into());

    assert!(engine.refresh("claude").await);
    assert!(!engine.inner.lock().await.cycle_running);
    assert_eq!(fetches.load(Ordering::SeqCst), 0);
}
