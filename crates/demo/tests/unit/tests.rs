//! Checks demo behavior through the public API.

use super::*;

#[test]
fn demo_has_every_provider() {
    assert_eq!(demo().len(), IDS.len());
}
#[test]
fn demo_windows_follow_macos_boundaries_and_change_with_time() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let first = demo_at(now);
    let later = demo_at(now + SignedDuration::from_mins(30));
    let session = &first["claude"].windows[0];
    assert_eq!(session.reset.as_deref(), Some("2026-10-12T18:00:00+00:00"));
    assert!(later["claude"].windows[0].used > session.used);
    assert!(
        first
            .values()
            .flat_map(|provider| &provider.windows)
            .all(|row| { row.reset.as_deref().unwrap().parse::<Timestamp>().unwrap() > now })
    );
    assert_eq!(first["claude"].windows[2].scope.as_deref(), Some("Fable"));
    assert_eq!(
        first["cursor"].spend.as_ref().unwrap().percent,
        Some(first["cursor"].windows[1].used)
    );
    assert_eq!(
        first["cursor"].spend.as_ref().unwrap().used,
        Some((first["cursor"].windows[1].used * 50.0).trunc() / 100.0)
    );
}
#[test]
fn demo_includes_reference_quota_windows() {
    let providers = demo();
    assert_eq!(
        providers["codex"]
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        [
            "session",
            "weekly",
            "additional:gpt-5.3-codex-spark:weekly",
            "code_review"
        ]
    );
    assert_eq!(
        providers["antigravity"]
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["gemini:session", "gemini:weekly", "3p:session", "3p:weekly"]
    );
}
#[test]
fn demo_uses_reference_credit_estimates() {
    let providers = demo();
    let codex = &providers["codex"];
    let renewal = codex
        .subscription_active_until
        .as_deref()
        .unwrap()
        .parse::<Timestamp>()
        .unwrap();
    assert!((renewal.as_second() - Timestamp::now().as_second() - 40 * 86_400).abs() <= 2);
    assert_eq!(codex.usage_credits.as_ref().unwrap().local_messages, Some((120, 260)));
    assert_eq!(codex.usage_credits.as_ref().unwrap().cloud_messages, Some((60, 130)));
    assert_eq!(codex.reset_credits.as_ref().unwrap().available, 1);
    assert_eq!(codex.reset_credits.as_ref().unwrap().total_earned, Some(3));
    assert_eq!(providers["gemini"].credit_balance, Some(1500.0));
}
#[test]
fn demo_includes_reference_provider_notices() {
    let providers = demo();
    assert_eq!(providers["claude"].notices[0].kind, NoticeKind::Promotion);
    assert_eq!(
        providers["codex"].notices[0].text,
        "Usage limits are doubled through the end of the month."
    );
    assert_eq!(providers["copilot"].notices[0].kind, NoticeKind::Info);
}
#[test]
fn demo_uses_synthetic_account_identities_and_reference_plans() {
    let providers = demo();
    assert_eq!(providers["claude"].identity, "you@example.com");
    assert_eq!(providers["claude"].plan, "Max 20x");
    assert_eq!(providers["antigravity"].plan, "Google AI Ultra");
    assert_eq!(providers["copilot"].identity, "octocat");
}

#[test]
fn demo_preserves_reference_metadata_and_refresh_time() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let providers = demo_at(now);
    let claude = &providers["claude"];
    assert_eq!(claude.updated, Some(now.as_second()));
    assert_eq!(claude.tier.as_deref(), Some("default_claude_max_20x"));
    assert_eq!(claude.local_usage.as_ref().unwrap().today_tokens, 6_200_000);
    assert_eq!(
        claude.local_usage.as_ref().unwrap().window_tokens,
        (1_840_000.0 * claude.windows[0].used / 60.0) as u64
    );
    assert_eq!(providers["antigravity"].tier.as_deref(), Some("google_ai_ultra"));
    assert!(providers["codex"].local_usage.is_none());
}

#[test]
fn long_text_demo_keeps_quota_values() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let standard = demo_at(now);
    let expanded = IDS
        .iter()
        .map(|id| (*id, demo_provider_at(id, now, true).unwrap()))
        .collect::<BTreeMap<_, _>>();
    for id in IDS {
        assert_eq!(expanded[id].windows.len(), standard[id].windows.len());
        assert_eq!(expanded[id].windows[0].used, standard[id].windows[0].used);
        assert!(expanded[id].windows[0].label.len() > 50);
        assert_eq!(expanded[id].notices.len(), standard[id].notices.len() + 1);
    }
    assert!(expanded["claude"].windows[2].scope.as_ref().unwrap().len() > 50);
    assert!(expanded["claude"].tier.as_ref().unwrap().len() > 50);
    assert!(expanded["codex"].tier.is_none());
}

#[test]
fn individual_demo_matches_full_snapshot_without_extra_providers() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let all = demo_at(now);
    for id in IDS {
        let provider = demo_provider_at(id, now, false).unwrap();
        assert_eq!(provider.id, all[id].id);
        assert_eq!(provider.windows.len(), all[id].windows.len());
        assert_eq!(provider.windows[0].used, all[id].windows[0].used);
        let expanded = demo_provider_at(id, now, true).unwrap();
        assert!(expanded.windows[0].label.len() > provider.windows[0].label.len());
    }
    assert!(demo_provider_at("unknown", now, false).is_none());
}
