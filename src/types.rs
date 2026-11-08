use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    Line,
    Word,
    Semantic,
    Json,
    Rss,
}

impl Default for DiffMode {
    fn default() -> Self {
        DiffMode::Line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WaitFor {
    #[default]
    Load,
    DomContentLoaded,
    /// Approximated as a short fixed grace period after `load` — chromiumoxide
    /// has no native CDP network-idle detector, unlike Playwright's
    /// `waitUntil: "networkidle"`. See README "JS-heavy pages".
    NetworkIdle,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOptions {
    #[serde(default)]
    pub wait_for: Option<WaitFor>,
    #[serde(default)]
    pub wait_for_selector: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderKind {
    #[default]
    Anthropic,
    Openai,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: String, // "added" | "removed" | "modified" | "unchanged"
    pub before: Option<String>,
    pub after: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    pub url: String,
    pub label: String,
    pub timestamp: DateTime<Utc>,
    pub changes: Vec<Change>,
    pub response_time_ms: Option<u64>,
    pub http_status: Option<u16>,
    #[serde(default)]
    pub ai_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub url: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceWindow {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBetween {
    pub from: String, // "09:00"
    pub to: String,   // "18:00"
    #[serde(default)]
    pub days: Vec<String>, // "monday".."sunday", empty = all days
    #[serde(default)]
    pub timezone: Option<String>, // IANA tz, default UTC
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FailurePolicy {
    #[serde(default)]
    pub consecutive_failures: Option<u32>,
    #[serde(default)]
    pub consecutive_successes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BasicAuth {
    pub user: String,
    pub pass: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAlertConfig {
    pub to: String,
    pub smtp: SmtpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchConfig {
    pub url: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub target: Option<String>, // CSS selector only in this port (see README)
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub schedule: Option<String>, // cron expr, overrides interval
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub ignore_selectors: Vec<String>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub proxies: Vec<String>,
    #[serde(default)]
    pub user_agents: Vec<String>,
    /// Render via headless Chromium (chromiumoxide) instead of a plain HTTP
    /// fetch. See README "JS-heavy pages".
    #[serde(default)]
    pub browser: bool,
    #[serde(default)]
    pub browser_options: Option<BrowserOptions>,
    /// Requires `browser: true`.
    #[serde(default)]
    pub screenshot_on_change: bool,
    #[serde(default)]
    pub diff_mode: DiffMode,
    #[serde(default)]
    pub webhooks: Vec<String>,
    #[serde(default = "default_min_changes")]
    pub min_changes: u32,
    #[serde(default)]
    pub cooldown: u64,
    #[serde(default)]
    pub retries: u32,
    #[serde(default = "default_retry_delay")]
    pub retry_delay_ms: u64,
    #[serde(default)]
    pub max_retry_delay_ms: Option<u64>,
    #[serde(default)]
    pub jitter: f64,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub max_snapshots: Option<usize>,
    #[serde(default)]
    pub change_threshold: f64,
    #[serde(default)]
    pub ignore_numbers: bool,
    #[serde(default)]
    pub alert_if_no_change_after: Option<u64>,
    #[serde(default)]
    pub webhook_retries: Option<u32>,
    #[serde(default)]
    pub archive_html: bool,
    #[serde(default)]
    pub change_spike_window: Option<u64>,
    #[serde(default)]
    pub change_spike_threshold: Option<u32>,
    #[serde(default)]
    pub expected_status: Option<u16>,
    #[serde(default)]
    pub track_response_time: bool,
    #[serde(default)]
    pub alert_on_status_change: bool,
    #[serde(default)]
    pub json_path: Option<String>,
    #[serde(default)]
    pub cookies: HashMap<String, String>,
    #[serde(default)]
    pub maintenance_windows: Vec<MaintenanceWindow>,
    #[serde(default)]
    pub active_between: Option<ActiveBetween>,
    #[serde(default)]
    pub failure_policy: Option<FailurePolicy>,
    #[serde(default)]
    pub alert_on_first_check: bool,
    #[serde(default)]
    pub webhook_headers: HashMap<String, String>,
    #[serde(default)]
    pub retention_days: Option<u32>,
    #[serde(default)]
    pub auth: Option<BasicAuth>,
    #[serde(default)]
    pub request_body: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub min_interval: Option<u64>,
    #[serde(default)]
    pub confirm_after: Option<u64>,
    #[serde(default)]
    pub email: Option<EmailAlertConfig>,
    #[serde(default)]
    pub ai_summary: bool,
    #[serde(default)]
    pub ai_provider: AiProviderKind,
    /// Falls back to `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`/`GEMINI_API_KEY`
    /// (matching `ai_provider`) when unset.
    #[serde(default)]
    pub ai_api_key: Option<String>,
    #[serde(default)]
    pub ai_prompt: Option<String>,
    #[serde(default)]
    pub allow_private_urls: bool,
    #[serde(default)]
    pub max_response_size: Option<u64>,
    #[serde(default)]
    pub paused: bool,
}

fn default_interval() -> u64 {
    300
}
fn default_timeout() -> u64 {
    15000
}
fn default_min_changes() -> u32 {
    1
}
fn default_retry_delay() -> u64 {
    1000
}

impl WatchConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            interval: default_interval(),
            timeout_ms: default_timeout(),
            min_changes: default_min_changes(),
            retry_delay_ms: default_retry_delay(),
            ..Default::default()
        }
    }

    pub fn effective_label(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.url.clone())
    }

    pub fn key(&self) -> String {
        self.id.clone().unwrap_or_else(|| self.url.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherStatus {
    pub url: String,
    pub id: Option<String>,
    pub label: String,
    pub target: Option<String>,
    pub interval: u64,
    pub paused: bool,
    pub in_maintenance: bool,
    pub last_check_at: Option<DateTime<Utc>>,
    pub next_check_at: Option<DateTime<Utc>>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub checks_count: u64,
    pub changes_count: u64,
    pub errors_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpikeInfo {
    pub url: String,
    pub label: String,
    pub changes_in_window: u32,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceInfo {
    pub url: String,
    pub label: String,
    pub seconds_since_last_change: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusChangeInfo {
    pub url: String,
    pub label: String,
    pub previous_status: u16,
    pub current_status: u16,
}
