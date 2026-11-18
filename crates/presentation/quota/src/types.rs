//! Defines serializable provider, quota, and settings values.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use quotaline_catalog::{CODES, IDS, REFRESH_POLICY, login, name};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Window {
    pub id: String,
    pub label: String,
    pub used: f64,
    #[serde(default = "active_window")]
    pub active: bool,
    pub reset: Option<String>,
    #[serde(default)]
    pub reset_precision: ResetPrecision,
    pub group: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default)]
    pub detail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pace: Option<crate::pace::PaceEstimate>,
}

fn active_window() -> bool {
    true
}

pub use quotaline_text::time::ResetPrecision;

impl Window {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        used: f64,
        reset: Option<String>,
        group: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            used: used.clamp(0.0, 100.0),
            active: true,
            reset,
            reset_precision: ResetPrecision::Instant,
            group: group.into(),
            scope: None,
            severity: None,
            detail: String::new(),
            help: String::new(),
            duration_seconds: None,
            pace: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialHealth {
    #[default]
    Unchecked,
    Missing,
    Valid,
    Expired,
    Unreadable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub credential_usable: bool,
    #[serde(default)]
    pub credential_health: CredentialHealth,
    #[serde(default)]
    pub recovery_status: Option<String>,
    pub plan: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    pub identity: String,
    #[serde(default)]
    pub organization: String,
    #[serde(skip)]
    pub account_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_active_until: Option<String>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_selection: Option<AccountSelection>,
    pub windows: Vec<Window>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_usage: Option<LocalUsage>,
    #[serde(default)]
    pub spend: Option<SpendControl>,
    #[serde(default)]
    pub details: Vec<Detail>,
    #[serde(default)]
    pub credit_balance: Option<f64>,
    #[serde(default)]
    pub usage_credits: Option<UsageCredits>,
    #[serde(default)]
    pub reset_credits: Option<ResetCredits>,
    #[serde(default)]
    pub cloud_credits: Option<CloudCredits>,
    #[serde(default)]
    pub reset_credit_expiries: Vec<CreditExpiry>,
    #[serde(default)]
    pub notices: Vec<Notice>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub notice: String,
    pub updated: Option<i64>,
    #[serde(default)]
    pub retry_after: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CreditExpiry {
    pub id: String,
    pub at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSelection {
    pub count: u32,
    pub label: String,
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "ProviderLocalUsageRecord"))]
pub struct LocalUsage {
    pub window_tokens: u64,
    pub window_cost: f64,
    pub cost_per_hour: f64,
    pub today_tokens: u64,
    pub today_cost: f64,
    pub today_messages: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Detail {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub value: String,
    #[serde(default = "default_detail_explanation")]
    pub explanation: String,
}

fn default_detail_explanation() -> String {
    "Provider-reported detail.".into()
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "ProviderCreditAllowanceRecord"))]
pub struct CreditAllowance {
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageCredits {
    pub unlimited: bool,
    pub has_credits: Option<bool>,
    pub overage_limit_reached: bool,
    pub local_messages: Option<(i64, i64)>,
    pub cloud_messages: Option<(i64, i64)>,
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "ProviderResetCreditsRecord"))]
pub struct ResetCredits {
    pub available: u64,
    pub applicable: u64,
    pub total_earned: Option<u64>,
    pub purchase_eligible: bool,
    pub details: Vec<ResetCreditDetail>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ResetCreditDetail {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub status: Option<String>,
    pub credit_type: Option<String>,
    pub description: Option<String>,
    pub granted_at: Option<String>,
    pub expires_at: Option<String>,
    #[serde(default = "default_reset_credit_explanation")]
    pub explanation: String,
}

pub fn default_reset_credit_explanation() -> String {
    "Provider-reported reset credit. Reading this information never reserves or consumes the credit.".into()
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "ProviderCloudCreditsRecord"))]
pub struct CloudCredits {
    pub limit: f64,
    pub remaining: f64,
    pub expires_at: Option<String>,
    pub locked_reason: Option<String>,
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "ProviderSpendRecord"))]
pub struct SpendControl {
    pub enabled: Option<bool>,
    #[serde(default)]
    pub can_toggle: bool,
    #[serde(default)]
    pub disabled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason_code: Option<String>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_money: Option<MoneyAmount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_money: Option<MoneyAmount>,
    pub percent: Option<f64>,
    pub currency: Option<String>,
    pub reset: Option<String>,
    pub limit_reached: bool,
    #[serde(default)]
    pub balance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_money: Option<MoneyAmount>,
    #[serde(default)]
    pub auto_reload: Option<bool>,
    #[serde(default)]
    pub can_purchase_credits: bool,
    #[serde(default)]
    pub credit_allowance: Option<CreditAllowance>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MoneyAmount {
    pub amount_minor: i64,
    pub currency: String,
    pub exponent: i32,
}

impl MoneyAmount {
    pub fn value(&self) -> f64 {
        self.amount_minor as f64 / 10_f64.powi(self.exponent)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    Promotion,
    LimitReached,
    SpendControl,
    Info,
}

impl NoticeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Promotion => "promotion",
            Self::LimitReached => "limitReached",
            Self::SpendControl => "spendControl",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

impl Notice {
    pub fn new(kind: NoticeKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            window_id: None,
            resets_at: None,
        }
    }
}

impl Provider {
    pub fn empty(id: &str, status: &str) -> Self {
        Self {
            id: id.into(),
            name: name(id).into(),
            status: status.into(),
            credential_usable: false,
            credential_health: CredentialHealth::Unchecked,
            recovery_status: None,
            plan: String::new(),
            tier: None,
            identity: String::new(),
            organization: String::new(),
            account_fingerprint: None,
            subscription_active_until: None,
            source: String::new(),
            account_selection: None,
            windows: Vec::new(),
            local_usage: None,
            spend: None,
            details: Vec::new(),
            credit_balance: None,
            usage_credits: None,
            reset_credits: None,
            cloud_credits: None,
            reset_credit_expiries: Vec::new(),
            notices: Vec::new(),
            warnings: Vec::new(),
            notice: format!("Run {}", login(id)),
            updated: None,
            retry_after: 0,
        }
    }

    pub fn connected(id: &str, plan: String, source: String, identity: String, windows: Vec<Window>) -> Self {
        Self {
            id: id.into(),
            name: name(id).into(),
            status: "connected".into(),
            credential_usable: true,
            credential_health: CredentialHealth::Valid,
            recovery_status: None,
            plan,
            tier: None,
            identity,
            organization: String::new(),
            account_fingerprint: None,
            subscription_active_until: None,
            source,
            account_selection: None,
            windows,
            local_usage: None,
            spend: None,
            details: Vec::new(),
            credit_balance: None,
            usage_credits: None,
            reset_credits: None,
            cloud_credits: None,
            reset_credit_expiries: Vec::new(),
            notices: Vec::new(),
            warnings: Vec::new(),
            notice: String::new(),
            updated: Some(Timestamp::now().as_second()),
            retry_after: 0,
        }
    }

    /// Whether paid credits are left: what the provider says, else an unlimited plan, else a positive balance.
    pub fn has_credits(&self) -> Option<bool> {
        let usage = self.usage_credits.as_ref();
        usage
            .and_then(|credits| credits.has_credits)
            .or_else(|| usage.is_some_and(|credits| credits.unlimited).then_some(true))
            .or_else(|| self.credit_balance.map(|balance| balance > 0.0))
    }

    pub fn account_fingerprint(&self) -> Option<String> {
        self.account_fingerprint.clone().or_else(|| {
            self.identity
                .contains('@')
                .then(|| account_fingerprint(&self.identity, &self.organization))
        })
    }
}

pub fn account_fingerprint(email: &str, organization: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{}|{organization}", email.to_lowercase()).as_bytes(),
    ))
}

#[cfg(test)]
#[path = "../tests/unit/types/tests.rs"]
mod tests;
