//! Creates representative provider snapshots and analytics for previews and screenshots.

use jiff::tz::Offset;
use jiff::{SignedDuration, Timestamp};
use quotaline_quota::*;
use std::collections::BTreeMap;

mod analytics;
pub use analytics::demo_analytics_at;

pub fn demo() -> BTreeMap<String, Provider> {
    demo_at(Timestamp::now())
}

pub fn demo_at(now: Timestamp) -> BTreeMap<String, Provider> {
    demo_at_filtered(now, None)
}

pub fn demo_provider_at(id: &str, now: Timestamp, long_text: bool) -> Option<Provider> {
    let mut provider = demo_at_filtered(now, Some(id)).remove(id)?;
    if long_text {
        expand_long_text(&mut provider);
    }
    Some(provider)
}

fn demo_at_filtered(now: Timestamp, selected: Option<&str>) -> BTreeMap<String, Provider> {
    let rows = [
        (
            "claude",
            "Max 20x",
            vec![
                ("session", "Current session", "session", 18_000, 0, 1.15, None),
                ("weekly", "All models", "weekly", 604_800, 288_000, 0.9, None),
                ("weekly:fable", "Fable", "weekly", 604_800, 288_000, 1.05, Some("Fable")),
                (
                    "weekly:sonnet",
                    "Sonnet",
                    "weekly",
                    604_800,
                    288_000,
                    0.4,
                    Some("Sonnet"),
                ),
            ],
        ),
        (
            "codex",
            "Pro",
            vec![
                ("session", "5-hour", "session", 18_000, 0, 0.7, None),
                ("weekly", "Weekly", "weekly", 604_800, 482_400, 0.95, None),
                (
                    "additional:gpt-5.3-codex-spark:weekly",
                    "GPT-5.3-Codex-Spark Weekly",
                    "weekly",
                    604_800,
                    468_000,
                    0.2,
                    None,
                ),
                ("code_review", "Code review", "other", 604_800, 172_800, 0.35, None),
            ],
        ),
        (
            "gemini",
            "Google AI Pro",
            vec![
                (
                    "model:gemini-2.5-pro",
                    "Gemini 2.5 Pro",
                    "other",
                    86_400,
                    25_200,
                    0.9,
                    None,
                ),
                (
                    "model:gemini-2.5-flash",
                    "Gemini 2.5 Flash",
                    "other",
                    86_400,
                    25_200,
                    0.3,
                    None,
                ),
            ],
        ),
        (
            "antigravity",
            "Google AI Ultra",
            vec![
                ("gemini:session", "Gemini 5-hour", "session", 18_000, 0, 0.75, None),
                ("gemini:weekly", "Gemini weekly", "weekly", 604_800, 388_800, 0.6, None),
                ("3p:session", "Claude and GPT 5-hour", "session", 18_000, 0, 1.05, None),
                (
                    "3p:weekly",
                    "Claude and GPT weekly",
                    "weekly",
                    604_800,
                    388_800,
                    0.85,
                    None,
                ),
            ],
        ),
        (
            "cursor",
            "Pro",
            vec![
                ("plan", "Plan usage", "monthly", 2_592_000, 0, 0.8, None),
                ("on_demand", "On-demand", "monthly", 2_592_000, 0, 0.25, None),
            ],
        ),
        (
            "copilot",
            "Pro",
            vec![(
                "premium_interactions",
                "Premium requests",
                "monthly",
                2_592_000,
                0,
                1.1,
                None,
            )],
        ),
    ];
    rows.into_iter()
        .filter(|(id, _, _)| selected.is_none_or(|wanted| wanted == *id))
        .map(|(id, plan, windows)| {
            let mapped = windows.into_iter().map(|spec| demo_window(spec, now)).collect();
            let mut provider = Provider::connected(
                id,
                plan.into(),
                "Demo data".into(),
                if id == "copilot" { "octocat" } else { "you@example.com" }.into(),
                mapped,
            );
            provider.updated = Some(now.as_second());
            provider.tier = match id {
                "claude" => Some("default_claude_max_20x".into()),
                "antigravity" => Some("google_ai_ultra".into()),
                _ => None,
            };
            provider.spend = match id {
                "claude" => Some(SpendControl {
                    enabled: Some(true),
                    can_toggle: true,
                    disabled_reason: None,
                    disabled_reason_code: None,
                    used: Some(12.4),
                    limit: Some(50.0),
                    used_money: None,
                    limit_money: None,
                    percent: Some(24.8),
                    currency: Some("USD".into()),
                    reset: Some(iso(Timestamp::from_second(demo_boundary(
                        now.as_second(),
                        2_592_000,
                        1_036_800,
                    ))
                    .unwrap())),
                    limit_reached: false,
                    balance: Some(37.6),
                    balance_money: None,
                    auto_reload: Some(false),
                    can_purchase_credits: true,
                    credit_allowance: None,
                }),
                "codex" => Some(SpendControl {
                    enabled: Some(true),
                    can_toggle: false,
                    disabled_reason: None,
                    disabled_reason_code: None,
                    used: None,
                    limit: Some(200.0),
                    used_money: None,
                    limit_money: None,
                    percent: None,
                    currency: Some("USD".into()),
                    reset: None,
                    limit_reached: false,
                    balance: None,
                    balance_money: None,
                    auto_reload: None,
                    can_purchase_credits: false,
                    credit_allowance: None,
                }),
                "cursor" => Some(SpendControl {
                    enabled: Some(true),
                    can_toggle: false,
                    disabled_reason: None,
                    disabled_reason_code: None,
                    used: Some((provider.windows[1].used * 50.0).trunc() / 100.0),
                    limit: Some(50.0),
                    used_money: None,
                    limit_money: None,
                    percent: Some(provider.windows[1].used),
                    currency: Some("USD".into()),
                    reset: provider.windows[1].reset.clone(),
                    limit_reached: false,
                    balance: None,
                    balance_money: None,
                    auto_reload: None,
                    can_purchase_credits: false,
                    credit_allowance: None,
                }),
                _ => None,
            };
            provider.notice = "Demo data".into();
            if id == "claude" {
                let session = &provider.windows[0];
                provider.local_usage = Some(LocalUsage {
                    window_tokens: (1_840_000.0 * session.used / 60.0) as u64,
                    window_cost: 22.4 * session.used / 60.0,
                    cost_per_hour: 8.1,
                    today_tokens: 6_200_000,
                    today_cost: 71.3,
                    today_messages: 214,
                });
                provider.cloud_credits = Some(CloudCredits {
                    limit: 250.0,
                    remaining: 83.0,
                    expires_at: Some(iso(now + days(24))),
                    locked_reason: None,
                });
            }
            provider.notices = match id {
                "claude" => vec![Notice::new(
                    NoticeKind::Promotion,
                    "Weekly limits are boosted 2x until the next reset.",
                )],
                "codex" => vec![Notice::new(
                    NoticeKind::Promotion,
                    "Usage limits are doubled through the end of the month.",
                )],
                "copilot" => vec![Notice::new(
                    NoticeKind::Info,
                    "Chat and completions are unlimited on this plan.",
                )],
                _ => Vec::new(),
            };
            if id == "codex" {
                provider.subscription_active_until = Some(iso(now + days(40)));
                provider.credit_balance = Some(42.5);
                provider.usage_credits = Some(UsageCredits {
                    unlimited: false,
                    has_credits: Some(true),
                    overage_limit_reached: false,
                    local_messages: Some((120, 260)),
                    cloud_messages: Some((60, 130)),
                });
                provider.reset_credits = Some(ResetCredits {
                    available: 1,
                    applicable: 1,
                    total_earned: Some(3),
                    purchase_eligible: true,
                    details: vec![ResetCreditDetail {
                        id: "bonus-reset".into(),
                        title: "Bonus reset".into(),
                        status: Some("Available".into()),
                        credit_type: Some("Session".into()),
                        description: Some("One extra session reset".into()),
                        granted_at: Some(iso(now - days(1))),
                        expires_at: Some(iso(now + days(3))),
                        explanation: default_reset_credit_explanation(),
                    }],
                });
                provider.reset_credit_expiries.push(CreditExpiry {
                    id: "bonus-reset".into(),
                    at: (now + days(3)).as_second(),
                });
            }
            if id == "gemini" {
                provider.credit_balance = Some(1500.0);
            }
            (id.into(), provider)
        })
        .collect()
}

fn expand_long_text(provider: &mut Provider) {
    let suffix = "verification-fixture-with-a-deliberately-long-identifier";
    provider.plan = format!("{} plan for a large multi-team organization", provider.plan);
    provider.tier = provider.tier.take().map(|tier| format!("{tier}-{suffix}"));
    provider.identity = "automation-account-with-a-long-address@engineering.example.com".into();
    provider.organization = "Example Engineering Platform and Developer Experience Organization".into();
    for window in &mut provider.windows {
        window.id = format!("{}-{suffix}", window.id);
        window.label = format!("{} usage window with a deliberately long model name", window.label);
        window.scope = window
            .scope
            .take()
            .map(|scope| format!("{scope} model scope with a deliberately long identifier"));
    }
    provider.notices.push(Notice::new(
        NoticeKind::Info,
        "Verification warning text is intentionally long so wrapping, accessibility values, and panel sizing can be checked without exposing account data.",
    ));
}

type DemoWindowSpec<'a> = (&'a str, &'a str, &'a str, i64, i64, f64, Option<&'a str>);

fn demo_window((id, label, group, duration, offset, pace, scope): DemoWindowSpec<'_>, now: Timestamp) -> Window {
    let reset = demo_boundary(now.as_second(), duration, offset);
    let mut spent = 0.0;
    let mut total = 0.0;
    let step = (duration / 48).max(900);
    for at in (reset - duration..reset).step_by(step as usize) {
        let weight = demo_intensity(at);
        total += weight;
        if at <= now.as_second() {
            spent += weight;
        }
    }
    let mut row = Window::new(
        id,
        label,
        spent / total * pace * 100.0,
        Some(iso(Timestamp::from_second(reset).unwrap())),
        group,
    );
    row.duration_seconds = Some(duration as u64);
    row.scope = scope.map(str::to_owned);
    row
}

fn days(count: i64) -> SignedDuration {
    SignedDuration::from_hours(count * 24)
}

fn iso(at: Timestamp) -> String {
    at.display_with_offset(Offset::UTC).to_string()
}

fn demo_boundary(now: i64, duration: i64, offset: i64) -> i64 {
    ((now - offset).div_euclid(duration) + 1) * duration + offset
}

fn demo_intensity(at: i64) -> f64 {
    let hour = at.rem_euclid(86_400) / 3_600;
    let workday = match hour {
        9..12 | 14..18 => 1.0,
        12..14 => 0.55,
        18..22 => 0.35,
        8 => 0.5,
        _ => 0.05,
    };
    let weekday = (at.div_euclid(86_400) + 4).rem_euclid(7);
    workday * if weekday == 0 || weekday == 6 { 0.3 } else { 1.0 }
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
