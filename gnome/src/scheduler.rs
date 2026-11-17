//! Schedules quota refreshes and local Claude transcript scans.

use super::*;

impl App {
    pub(super) async fn scheduled(&self) {
        let due = {
            let inner = self.inner.lock().await;
            if inner.settings.demo {
                return;
            }
            let schedules = self.schedules(&inner);
            let inputs = self.schedule_inputs(&inner, &schedules);
            // Each cycle wakes the scheduler when it ends, so a cycle with nothing due would start another.
            inner
                .session
                .next_deadline(&inputs)
                .is_some_and(|deadline| deadline <= inputs.now)
        };
        if due {
            self.request(refresh::RefreshIntent {
                reason: refresh::RefreshReason::Scheduled,
                usage: refresh::RefreshPolicy::IfDue,
                analytics: refresh::RefreshPolicy::Skip,
                providers: None,
            })
            .await;
        }
        let retention_days = {
            let mut inner = self.inner.lock().await;
            let interval =
                Duration::from_secs(u64::from(inner.settings.analytics_interval_minutes) * self.minute_seconds);
            if inner.settings.providers.get("codex") != Some(&false)
                && inner.last_analytics.is_none_or(|last| last.elapsed() >= interval)
            {
                inner.last_analytics = Some(Instant::now());
                Some(inner.settings.retention_days)
            } else {
                None
            }
        };
        if let Some(retention_days) = retention_days {
            let app = self.clone();
            tokio::spawn(async move {
                let stored = blocking(store::load_codex_watermarks);
                if let Some(analytics) =
                    providers::codex_analytics(&app.client, &app.home, retention_days, stored.as_deref()).await
                {
                    app.store_codex_analytics(analytics);
                }
            });
        }
        self.scan_claude().await;
    }

    fn store_codex_analytics(&self, analytics: providers::CodexAnalytics) {
        if let Some(snapshot) = &analytics.snapshot {
            match blocking(|| store::record_snapshot(snapshot)) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("Codex analytics discarded after account change");
                    return;
                }
                Err(error) => {
                    eprintln!("Codex analytics write failed: {error}");
                    return;
                }
            }
        }
        if let Some(watermarks) = analytics.watermarks
            && let Err(error) = blocking(|| store::save_codex_watermarks(&watermarks))
        {
            eprintln!("Codex analytics progress write failed: {error}");
        }
    }

    pub(super) async fn next_scheduled(&self) -> Option<Instant> {
        let inner = self.inner.lock().await;
        if inner.settings.demo {
            return None;
        }
        let now = Instant::now();
        let mut deadlines = Vec::with_capacity(3);
        // A running cycle reschedules when it finishes; its providers still look due until then.
        if !inner.cycle_running {
            let schedules = self.schedules(&inner);
            let epoch = epoch();
            deadlines.extend(
                inner
                    .session
                    .next_deadline(&self.schedule_inputs(&inner, &schedules))
                    .map(|deadline| now + Duration::from_secs_f64((deadline - epoch).max(0.0))),
            );
        }
        if inner.settings.providers.get("codex") != Some(&false) {
            deadlines.push(inner.last_analytics.map_or(now, |last| {
                last + Duration::from_secs(u64::from(inner.settings.analytics_interval_minutes) * self.minute_seconds)
            }));
        }
        if inner.settings.providers.get("claude") != Some(&false) && !inner.claude_scanning {
            let seconds = if inner.claude_scan_pending {
                1
            } else {
                u64::from(inner.settings.analytics_interval_minutes) * 60
            };
            deadlines.push(
                inner
                    .last_claude_scan
                    .map_or(now, |last| last + Duration::from_secs(seconds)),
            );
        }
        deadlines.into_iter().min()
    }

    pub(super) fn schedules(&self, inner: &Inner) -> Vec<refresh::ProviderSchedule<'static>> {
        let minute = self.minute_seconds as f64;
        REFRESH_POLICY
            .into_iter()
            .map(|(id, minimum, _)| refresh::ProviderSchedule {
                id,
                active: inner.settings.providers.get(id) != Some(&false)
                    && !refresh::needs_credentials(inner.providers.get(id)),
                minimum: f64::from(minimum) * minute,
                active_interval: f64::from(minimum) * minute,
                requested: f64::from(inner.settings.interval_minutes[id]) * minute,
                supports_analytics: false,
            })
            .collect()
    }

    pub(super) fn schedule_inputs<'a>(
        &self,
        inner: &Inner,
        schedules: &'a [refresh::ProviderSchedule<'static>],
    ) -> refresh::ScheduleInputs<'a> {
        let minute = self.minute_seconds as f64;
        refresh::ScheduleInputs {
            providers: schedules,
            visible: inner.panel_open,
            throttled: inner.power_saver || inner.thermal_pressure,
            analytics_interval: f64::from(inner.settings.analytics_interval_minutes) * minute,
            maximum: f64::from(quotaline_catalog::MAXIMUM_REFRESH_MINUTES) * minute,
            idle_threshold: self.idle_seconds as f64,
            now: epoch(),
        }
    }

    pub(super) async fn watch_power_profiles(&self) -> zbus::Result<()> {
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
        .take_until(self.power_stop.notified())
        .try_for_each(|(is_profile, change)| async move {
            let value = change.get().await;
            let mut inner = self.inner.lock().await;
            let previous = if is_profile {
                inner.power_saver
            } else {
                inner.thermal_pressure
            };
            let throttled = value.map_or(previous, |value| {
                if is_profile {
                    value == "power-saver"
                } else {
                    value == "high-operating-temperature"
                }
            });
            if previous != throttled {
                if is_profile {
                    inner.power_saver = throttled;
                } else {
                    inner.thermal_pressure = throttled;
                }
                self.client
                    .set_local_scan_throttled(inner.power_saver || inner.thermal_pressure);
                self.schedule_changed.notify_one();
            }
            Ok(())
        })
        .await
    }

    pub(super) async fn scan_claude(&self) {
        let (days, window_start, discover) = {
            let mut inner = self.inner.lock().await;
            if inner.settings.demo || inner.settings.providers.get("claude") == Some(&false) || inner.claude_scanning {
                return;
            }
            let interval = if inner.claude_scan_pending {
                1
            } else {
                u64::from(inner.settings.analytics_interval_minutes) * 60
            };
            if inner
                .last_claude_scan
                .is_some_and(|last| last.elapsed() < Duration::from_secs(interval))
            {
                return;
            }
            inner.last_claude_scan = Some(Instant::now());
            inner.claude_scanning = true;
            let now = Timestamp::now().as_second();
            let start = inner
                .providers
                .get("claude")
                .and_then(|provider| provider.windows.iter().find(|row| row.id == "session"))
                .and_then(|row| {
                    row.reset
                        .as_deref()
                        .and_then(|text| text.parse::<Timestamp>().ok())
                        .map(|reset| reset.as_second() - row.duration_seconds.unwrap_or(18_000) as i64)
                })
                .unwrap_or(now - 18_000);
            (
                inner.settings.retention_days,
                start,
                !inner.claude_scanned || !(inner.power_saver || inner.thermal_pressure),
            )
        };
        let app = self.clone();
        tokio::spawn(async move {
            let reader = app.claude_reader.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut reader = reader.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let scan = reader.scan(days, window_start, discover);
                claude_transcripts::persist(&scan).map(|()| (scan.pending, scan.usage))
            })
            .await;
            let mut inner = app.inner.lock().await;
            inner.claude_scanning = false;
            match result {
                Ok(Ok((pending, usage))) => {
                    inner.claude_scanned = true;
                    inner.claude_scan_pending = pending;
                    inner.claude_local_usage = usage;
                }
                failed => {
                    inner.claude_scan_pending = true;
                    eprintln!("Claude transcript scan failed: {failed:?}");
                }
            }
            drop(inner);
            app.schedule_changed.notify_one();
            app.emit().await;
        });
    }
}
