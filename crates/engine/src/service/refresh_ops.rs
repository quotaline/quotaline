//! Refreshes providers and persists their current snapshots.

use super::*;

impl Engine {
    pub async fn refresh(&self, requested: &str) -> bool {
        if requested != "all" && !IDS.contains(&requested) {
            return false;
        }
        if self.delegate_refresh(requested).await {
            return true;
        }
        self.request(refresh::RefreshIntent {
            reason: refresh::RefreshReason::UserInitiated,
            usage: refresh::RefreshPolicy::Force,
            analytics: refresh::RefreshPolicy::Skip,
            providers: (requested != "all").then(|| BTreeSet::from([requested.to_owned()])),
        })
        .await;
        true
    }

    pub(super) async fn request(&self, intent: refresh::RefreshIntent) {
        {
            let mut inner = self.inner.lock().await;
            if inner.settings.demo {
                inner.providers = demo();
                inner.statuses.clear();
                inner.last_refresh = Some(Timestamp::now().as_second());
                drop(inner);
                self.persist().await;
                self.emit().await;
                return;
            }
            // Fetches in flight answer a request for only their providers, as they finish after it was made.
            if intent
                .providers
                .as_ref()
                .is_some_and(|providers| providers.is_subset(&inner.running))
            {
                return;
            }
            inner.session.request(intent);
            if inner.cycle_running {
                return;
            }
            inner.cycle_running = true;
        }
        let app = self.clone();
        tokio::spawn(async move { app.drain().await });
    }

    // Runs one coalesced request at a time, as the macOS coordinator does, so a request made during a cycle waits
    // for it instead of fetching the same provider twice.
    async fn drain(&self) {
        loop {
            let (cycle, plans, allow_token_refresh) = {
                let mut inner = self.inner.lock().await;
                let Some(cycle) = inner.session.start() else {
                    inner.cycle_running = false;
                    drop(inner);
                    // The scheduler skips refresh deadlines while a cycle runs, so it must look again now.
                    self.schedule_changed.notify_one();
                    break;
                };
                let schedules = self.schedules(&inner);
                let mut plans = inner.session.plan(&cycle, &self.schedule_inputs(&inner, &schedules));
                // A refresh the user asks for reaches an inactive provider, but not one turned off since it was queued.
                plans
                    .fetches
                    .retain(|fetch| inner.settings.providers.get(&fetch.provider) != Some(&false));
                if plans.fetches.is_empty() {
                    inner.session.finish(&cycle);
                }
                inner
                    .running
                    .extend(plans.fetches.iter().map(|fetch| fetch.provider.clone()));
                (cycle, plans, inner.settings.allow_token_refresh)
            };
            self.emit().await;
            let fetches = plans.fetches.into_iter().map(|fetch| {
                let app = self.clone();
                let cycle = cycle.clone();
                tokio::spawn(async move {
                    let started = Instant::now();
                    let _permit = app.slots.acquire().await.expect("refresh slots remain open");
                    let id = IDS
                        .into_iter()
                        .find(|id| *id == fetch.provider)
                        .expect("plans name known providers");
                    let outcome = providers::collect(&app.client, id, &app.home, allow_token_refresh).await;
                    app.finish(&cycle, &fetch, outcome, started.elapsed()).await;
                })
            });
            for fetch in fetches.collect::<Vec<_>>() {
                let _ = fetch.await;
            }
        }
    }

    async fn finish(
        &self,
        cycle: &refresh::Cycle,
        fetch: &refresh::PlannedFetch,
        outcome: Result<Provider, providers::Failure>,
        elapsed: Duration,
    ) {
        let id = fetch.provider.as_str();
        let mut notifications = Vec::new();
        let status = match &outcome {
            Ok(provider) => provider.status.clone(),
            Err(error) => error.status.to_string(),
        };
        let completed = epoch();
        let now = completed as i64;
        let fetched = match &outcome {
            Ok(provider) if provider.status == "stale" => refresh::FetchOutcome::Partial,
            Ok(_) => refresh::FetchOutcome::Success,
            Err(error) => match error.kind() {
                ProviderFailureKind::RateLimited => refresh::FetchOutcome::RateLimited,
                ProviderFailureKind::NotAuthenticated => refresh::FetchOutcome::NotAuthenticated,
                ProviderFailureKind::NetworkUnavailable => refresh::FetchOutcome::NetworkUnavailable,
                ProviderFailureKind::Failed => refresh::FetchOutcome::Failed,
            },
        };
        let twelve_hour = fetched == refresh::FetchOutcome::RateLimited && self.host.twelve_hour_clock().await;
        let clock = crate::clock::SessionClock::new(jiff::tz::TimeZone::system(), twelve_hour);
        let detailed = {
            let mut inner = self.inner.lock().await;
            inner.running.remove(id);
            // The last result ends the cycle before it is published, so a refresh asked for in reaction to it
            // starts a new cycle instead of joining this one.
            if inner.running.is_empty() {
                inner.session.finish(cycle);
            }
            let transition = inner.session.complete(refresh::Completion {
                generation: cycle.generation,
                provider: id,
                outcome: fetched,
                retry_after: outcome.as_ref().err().map(|error| error.retry_after as f64),
                retry_interval: fetch.retry_interval,
                included_analytics: fetch.include_analytics,
                now: completed,
            });
            // A cancelled cycle's result, or one for a provider switched off meanwhile, must not change state.
            let Some(transition) = transition.filter(|_| inner.settings.providers.get(id) != Some(&false)) else {
                drop(inner);
                self.emit().await;
                return;
            };
            if inner.running.is_empty() {
                inner.last_refresh = Some(now);
            }
            let previous = inner.providers.get(id).cloned();
            let previous_account = previous.as_ref().and_then(Provider::account_fingerprint);
            let fetched_account = outcome.as_ref().ok().and_then(Provider::account_fingerprint);
            let applied = refresh::apply(refresh::ApplyInput {
                outcome: fetched,
                message: match &outcome {
                    Ok(provider) => Some(provider.notice.as_str()),
                    Err(error) => Some(error.message.as_str()),
                },
                readings: refresh::Readings {
                    previous_account: previous_account.as_deref(),
                    previous_fetched_at: previous
                        .as_ref()
                        .and_then(|snapshot| snapshot.updated)
                        .map(|at| at as f64),
                    fetched_account: fetched_account.as_deref(),
                    fetched_at: outcome
                        .as_ref()
                        .ok()
                        .and_then(|provider| provider.updated)
                        .map(|at| at as f64),
                },
                transition,
                now: completed,
                clock: &clock,
            });
            let status = refresh::settle(
                &inner.statuses.get(id).cloned().unwrap_or_default(),
                refresh::SettleInput {
                    availability: applied.availability,
                    last_error: applied.last_error.as_deref(),
                    retry_not_before: applied.retry_not_before,
                    succeeded: applied.succeeded,
                    warnings: outcome
                        .as_ref()
                        .map(|provider| provider.warnings.clone())
                        .unwrap_or_default(),
                    included_analytics: fetch.include_analytics,
                    active: true,
                    now: completed,
                },
            );
            if id == "codex" && previous_account.is_some() && applied.account_changed {
                inner.last_analytics = None;
            }
            let mut provider = match outcome {
                Ok(fetched_provider) if applied.replace_snapshot => fetched_provider,
                Ok(fetched_provider) => previous.clone().unwrap_or(fetched_provider),
                Err(error) => {
                    let mut kept = previous.clone().unwrap_or_else(|| Provider::empty(id, error.status));
                    let old = kept.clone();
                    kept.credential_usable = error.credential_usable;
                    kept.credential_health = error.credential_health;
                    kept.recovery_status = Some(error.recovery_status.unwrap_or(error.status).into());
                    kept.notice = status.last_error.clone().unwrap_or_default();
                    kept.status = if kept.windows.is_empty() { error.status } else { "stale" }.into();
                    notifications.extend(
                        notifications::auth_problem(Some(&old), &kept, &inner.settings, error.credential_missing)
                            .into_iter()
                            .filter(|event| inner.notified.accept(event))
                            .map(|event| Planned {
                                provider: kept.name.clone(),
                                event,
                                sign_in: Some(id.to_owned()),
                                fetched_at: now,
                            }),
                    );
                    kept
                }
            };
            if fetched == refresh::FetchOutcome::Partial {
                provider.status = "stale".into();
            }
            provider.notice = status.last_error.clone().unwrap_or_default();
            provider.warnings = status.warnings.clone();
            provider.retry_after = status
                .retry_not_before
                .map_or(0, |retry| (retry - completed).max(0.0) as u64);
            if applied.record_history {
                let mut samples = if applied.account_changed {
                    Vec::new()
                } else {
                    blocking(|| store::recent_samples(id, now)).unwrap_or_default()
                };
                for row in &provider.windows {
                    samples.push((row.id.clone(), now, row.used));
                }
                notifications.extend(
                    notifications::plan(
                        previous.as_ref().filter(|_| !applied.account_changed),
                        &provider,
                        &inner.settings,
                        &samples,
                        now,
                    )
                    .into_iter()
                    .filter(|event| inner.notified.accept(event))
                    .map(|event| Planned {
                        provider: provider.name.clone(),
                        event,
                        sign_in: None,
                        fetched_at: provider.updated.unwrap_or(now),
                    }),
                );
                if let Err(error) = blocking(|| store::record(&provider)) {
                    eprintln!("History write for {id} failed: {error}");
                }
                samples.retain(|(_, at, _)| *at >= now - 3600);
                inner.recent_samples.insert(id.into(), samples);
            }
            inner.providers.insert(id.into(), provider);
            inner.statuses.insert(id.into(), status);
            inner.notified.trim();
            inner
                .settings
                .detailed_logging
                .then(|| format!("{id} refresh completed in {} ms", elapsed.as_millis()))
        };
        let planned = !notifications.is_empty();
        // Queued before the state change goes out, so a reader that sees the new reading also sees its notifications.
        if planned && !self.post_notifications(&notifications).await {
            self.inner.lock().await.pending_notifications.extend(notifications);
        }
        self.persist().await;
        self.log(
            if status == "connected" { "info" } else { "warning" },
            "refresh",
            &format!("{id} refresh: {status}"),
        )
        .await;
        if let Some(message) = detailed {
            self.log("debug", "refresh", &message).await;
        }
        if planned {
            self.host.notification_planned().await;
        }
        self.schedule_changed.notify_one();
        self.emit().await;
    }

    pub(super) async fn persist(&self) {
        let _gate = self.persistence.lock().await;
        let (snapshots, demo, export) = {
            let inner = self.inner.lock().await;
            (
                inner.providers.clone(),
                inner.settings.demo,
                inner.settings.write_usage_json,
            )
        };
        if !demo && let Err(error) = store::atomic_json(&store::snapshot(), &snapshots) {
            eprintln!("Snapshot save failed: {error}");
        }
        if export {
            let _ = store::atomic_json(&store::usage(), &export::UsageExport::from(&snapshots));
        }
    }
}
