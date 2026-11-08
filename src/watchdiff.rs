use crate::cleaner;
use crate::diff;
use crate::extra_watchers::ExtraWatcher;
use crate::fetcher;
use crate::store::{JsonStore, Store};
use crate::types::*;
use crate::webhook;
use chrono::{Datelike, Timelike, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

pub type ChangeCallback          = Arc<dyn Fn(DiffReport) + Send + Sync>;
pub type ErrorCallback           = Arc<dyn Fn(anyhow::Error, WatchConfig) + Send + Sync>;
pub type SilenceCallback         = Arc<dyn Fn(SilenceInfo) + Send + Sync>;
pub type SpikeCallback           = Arc<dyn Fn(SpikeInfo) + Send + Sync>;
pub type StatusChangeCallback    = Arc<dyn Fn(StatusChangeInfo) + Send + Sync>;
pub type RecoveryCallback        = Arc<dyn Fn(WatchConfig) + Send + Sync>;
/// Custom alert condition. Returning `false` suppresses the alert (the
/// change is still detected and stored) without consuming the cooldown.
pub type AlertIfCallback         = Arc<dyn Fn(&DiffReport) -> bool + Send + Sync>;
/// Async hook fired before every fetch tick (e.g. to refresh an auth
/// token). The returned map is merged into the request headers for *that
/// one check only* — `WatchConfig` itself stays immutable and shared across
/// ticks, so this is the mechanism for a hook to actually affect the
/// upcoming fetch rather than just running a side effect that nothing reads.
/// An error is logged as a warning but does not skip the check.
pub type OnBeforeCheckCallback   = Arc<dyn Fn(&WatchConfig) -> futures::future::BoxFuture<'static, anyhow::Result<HashMap<String, String>>> + Send + Sync>;
/// Computes the AI summary prompt per-report (overrides `WatchConfig.ai_prompt`
/// when set, mirroring the TS library's `aiPrompt: (report) => string`).
pub type AiPromptCallback        = Arc<dyn Fn(&DiffReport) -> String + Send + Sync>;

#[derive(Default)]
pub struct Handlers {
    pub on_change: Vec<ChangeCallback>,
    pub on_error: Option<ErrorCallback>,
    pub on_silence: Option<SilenceCallback>,
    pub on_spike: Option<SpikeCallback>,
    pub on_status_change: Option<StatusChangeCallback>,
    pub on_recovery: Option<RecoveryCallback>,
    pub alert_if: Option<AlertIfCallback>,
    pub on_before_check: Option<OnBeforeCheckCallback>,
    pub ai_prompt_fn: Option<AiPromptCallback>,
}

#[derive(Default, Clone)]
pub struct RuntimeState {
    pub paused: bool,
    pub last_check_at: Option<chrono::DateTime<Utc>>,
    pub next_check_at: Option<chrono::DateTime<Utc>>,
    pub last_change_at: Option<chrono::DateTime<Utc>>,
    pub checks_count: u64,
    pub changes_count: u64,
    pub errors_count: u64,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub alerts_suppressed: bool,
    pub last_alert_at: Option<chrono::DateTime<Utc>>,
    pub last_http_status: Option<u16>,
    pub change_timestamps: Vec<chrono::DateTime<Utc>>,
    pub last_spike_alert_at: Option<chrono::DateTime<Utc>>,
    pub last_silence_alert_at: Option<chrono::DateTime<Utc>>,
    pub first_check_done: bool,
}

struct WatcherEntry {
    config: WatchConfig,
    handlers: Handlers,
    runtime: Mutex<RuntimeState>,
}

pub struct WatchDiffOptions {
    pub storage_dir: String,
    pub store: Option<Arc<dyn Store>>,
    pub concurrency: Option<usize>,
    pub min_interval: Option<u64>,
    pub max_watchers: Option<usize>,
    /// Chromium executable for `browser: true` watchers (e.g. the path
    /// returned by `browser::fetch_chromium()`). Defaults to
    /// chromiumoxide's auto-detection of a system-installed browser.
    pub browser_executable: Option<std::path::PathBuf>,
}

impl Default for WatchDiffOptions {
    fn default() -> Self {
        Self {
            storage_dir: ".watchdiff".to_string(),
            store: None,
            concurrency: None,
            min_interval: None,
            max_watchers: None,
            browser_executable: None,
        }
    }
}

pub struct WatchDiff {
    store: Arc<dyn Store>,
    json_store_dir: String,
    watchers: Arc<Mutex<HashMap<String, Arc<WatcherEntry>>>>,
    global_on_change: Arc<Mutex<Vec<ChangeCallback>>>,
    semaphore: Option<Arc<Semaphore>>,
    min_interval: Option<u64>,
    max_watchers: Option<usize>,
    running: Arc<AtomicBool>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    extra_watchers: Mutex<Vec<Arc<dyn ExtraWatcher>>>,
    browser_pool: Arc<crate::browser::BrowserPool>,
}

impl WatchDiff {
    pub fn new(opts: WatchDiffOptions) -> anyhow::Result<Self> {
        let store: Arc<dyn Store> = match opts.store {
            Some(s) => s,
            None => Arc::new(JsonStore::new(&opts.storage_dir)?),
        };
        Ok(Self {
            store,
            json_store_dir: opts.storage_dir,
            watchers: Arc::new(Mutex::new(HashMap::new())),
            global_on_change: Arc::new(Mutex::new(Vec::new())),
            semaphore: opts.concurrency.map(|c| Arc::new(Semaphore::new(c))),
            min_interval: opts.min_interval,
            max_watchers: opts.max_watchers,
            running: Arc::new(AtomicBool::new(false)),
            tasks: Mutex::new(Vec::new()),
            extra_watchers: Mutex::new(Vec::new()),
            browser_pool: Arc::new(match opts.browser_executable {
                Some(path) => crate::browser::BrowserPool::with_executable(path),
                None => crate::browser::BrowserPool::new(),
            }),
        })
    }

    /// Registers a local file to monitor through the same diff pipeline as
    /// web pages (minus HTTP-specific features like headers or proxies).
    pub fn watch_file(&self, cfg: crate::extra_watchers::FileWatchConfig) {
        self.extra_watchers
            .lock()
            .unwrap()
            .push(Arc::new(crate::extra_watchers::FileWatcher::new(cfg)));
    }

    /// Registers a REST/JSON API endpoint. Convenience over `.watch()` that
    /// defaults `diffMode` to `json` and enables response-time tracking.
    pub fn watch_api(&self, mut config: WatchConfig) -> anyhow::Result<&Self> {
        if matches!(config.diff_mode, DiffMode::Line) {
            config.diff_mode = DiffMode::Json;
        }
        config.track_response_time = true;
        self.watch(config)
    }

    /// Registers an SSL/TLS certificate to monitor for replacement or
    /// upcoming expiry.
    pub fn watch_cert(&self, cfg: crate::extra_watchers::CertWatchConfig) {
        self.extra_watchers
            .lock()
            .unwrap()
            .push(Arc::new(crate::extra_watchers::CertWatcher::new(cfg)));
    }

    /// Registers a `sitemap.xml` URL to monitor for added/removed URLs.
    pub fn watch_sitemap(&self, cfg: crate::extra_watchers::SitemapWatchConfig) {
        self.extra_watchers
            .lock()
            .unwrap()
            .push(Arc::new(crate::extra_watchers::SitemapWatcher::new(cfg)));
    }

    /// Registers a database table to monitor. Only `sqlite://` connection
    /// strings are supported in this port (see README "Scope").
    pub fn watch_db(&self, cfg: crate::extra_watchers::DbWatchConfig) {
        self.extra_watchers
            .lock()
            .unwrap()
            .push(Arc::new(crate::extra_watchers::DbWatcher::new(cfg)));
    }

    pub fn watch(&self, config: WatchConfig) -> anyhow::Result<&Self> {
        let mut watchers = self.watchers.lock().unwrap();
        if let Some(max) = self.max_watchers {
            if watchers.len() >= max {
                anyhow::bail!("maxWatchers limit ({max}) reached");
            }
        }
        let mut cfg = config;
        if cfg.min_interval.is_none() {
            cfg.min_interval = self.min_interval;
        }
        if let Some(floor) = cfg.min_interval {
            if cfg.interval < floor {
                cfg.interval = floor;
            }
        }
        let key            = cfg.key();
        let mut runtime    = RuntimeState::default();
        runtime.paused     = cfg.paused;
        watchers.insert(
            key,
            Arc::new(WatcherEntry {
                config: cfg,
                handlers: Handlers::default(),
                runtime: Mutex::new(runtime),
            }),
        );
        Ok(self)
    }

    pub fn watch_with_handlers(&self, config: WatchConfig, handlers: Handlers) -> anyhow::Result<()> {
        let mut watchers = self.watchers.lock().unwrap();
        if let Some(max) = self.max_watchers {
            if watchers.len() >= max {
                anyhow::bail!("maxWatchers limit ({max}) reached");
            }
        }
        let mut cfg = config;
        if cfg.min_interval.is_none() {
            cfg.min_interval = self.min_interval;
        }
        if let Some(floor) = cfg.min_interval {
            if cfg.interval < floor {
                cfg.interval = floor;
            }
        }
        let key            = cfg.key();
        let mut runtime    = RuntimeState::default();
        runtime.paused     = cfg.paused;
        watchers.insert(
            key,
            Arc::new(WatcherEntry {
                config: cfg,
                handlers,
                runtime: Mutex::new(runtime),
            }),
        );
        Ok(())
    }

    pub fn on_change(&self, cb: ChangeCallback) -> &Self {
        self.global_on_change.lock().unwrap().push(cb);
        self
    }

    pub fn pause(&self, selector: &str) {
        self.set_paused(selector, true);
    }

    pub fn resume(&self, selector: &str) {
        self.set_paused(selector, false);
    }

    fn set_paused(&self, selector: &str, paused: bool) {
        let watchers = self.watchers.lock().unwrap();
        for entry in watchers.values() {
            let matches = entry.config.key() == selector
                || entry.config.url == selector
                || entry.config.tags.iter().any(|t| t == selector);
            if matches {
                entry.runtime.lock().unwrap().paused = paused;
            }
        }
    }

    pub fn status(&self) -> Vec<WatcherStatus> {
        let watchers = self.watchers.lock().unwrap();
        watchers
            .values()
            .map(|entry| {
                let rt = entry.runtime.lock().unwrap();
                WatcherStatus {
                    url: entry.config.url.clone(),
                    id: entry.config.id.clone(),
                    label: entry.config.effective_label(),
                    target: entry.config.target.clone(),
                    interval: entry.config.interval,
                    paused: rt.paused,
                    in_maintenance: in_maintenance_window(&entry.config, Utc::now()),
                    last_check_at: rt.last_check_at,
                    next_check_at: rt.next_check_at,
                    last_change_at: rt.last_change_at,
                    checks_count: rt.checks_count,
                    changes_count: rt.changes_count,
                    errors_count: rt.errors_count,
                }
            })
            .collect()
    }

    pub fn history(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<Vec<Snapshot>> {
        self.store.load_snapshots(url_or_id, limit)
    }

    pub fn reports(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<Vec<DiffReport>> {
        self.store.load_reports(url_or_id, limit)
    }

    pub fn clear(&self, url_or_id: &str) -> anyhow::Result<()> {
        self.store.clear(url_or_id)
    }

    pub fn export_reports_json(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<String> {
        let reports = self.store.load_reports(url_or_id, limit)?;
        Ok(serde_json::to_string_pretty(&reports)?)
    }

    pub fn export_snapshots_json(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<String> {
        let snapshots = self.store.load_snapshots(url_or_id, limit)?;
        Ok(serde_json::to_string_pretty(&snapshots)?)
    }

    pub fn export_reports_csv(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<String> {
        let reports    = self.store.load_reports(url_or_id, limit)?;
        let mut wtr    = csv::Writer::from_writer(vec![]);
        wtr.write_record(["timestamp", "url", "label", "kind", "before", "after", "context"])?;
        for r in reports {
            for c in &r.changes {
                wtr.write_record([
                    r.timestamp.to_rfc3339(),
                    r.url.clone(),
                    r.label.clone(),
                    c.kind.clone(),
                    c.before.clone().unwrap_or_default(),
                    c.after.clone().unwrap_or_default(),
                    c.context.clone().unwrap_or_default(),
                ])?;
            }
        }
        Ok(String::from_utf8(wtr.into_inner()?)?)
    }

    pub fn export_snapshots_csv(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<String> {
        let snapshots    = self.store.load_snapshots(url_or_id, limit)?;
        let mut wtr      = csv::Writer::from_writer(vec![]);
        wtr.write_record(["timestamp", "url", "http_status", "content"])?;
        for s in snapshots {
            wtr.write_record([
                s.timestamp.to_rfc3339(),
                s.url,
                s.http_status.map(|s| s.to_string()).unwrap_or_default(),
                s.content,
            ])?;
        }
        Ok(String::from_utf8(wtr.into_inner()?)?)
    }

    pub fn export_reports_xlsx(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<Vec<u8>> {
        use rust_xlsxwriter::Workbook;

        let reports = self.store.load_reports(url_or_id, limit)?;
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        for (col, header) in ["timestamp", "url", "label", "kind", "before", "after", "context"].iter().enumerate() {
            worksheet.write(0, col as u16, *header)?;
        }
        let mut row = 1u32;
        for r in reports {
            for c in &r.changes {
                worksheet.write(row, 0, r.timestamp.to_rfc3339())?;
                worksheet.write(row, 1, r.url.as_str())?;
                worksheet.write(row, 2, r.label.as_str())?;
                worksheet.write(row, 3, c.kind.as_str())?;
                worksheet.write(row, 4, c.before.as_deref().unwrap_or(""))?;
                worksheet.write(row, 5, c.after.as_deref().unwrap_or(""))?;
                worksheet.write(row, 6, c.context.as_deref().unwrap_or(""))?;
                row += 1;
            }
        }
        Ok(workbook.save_to_buffer()?)
    }

    pub fn export_snapshots_xlsx(&self, url_or_id: &str, limit: Option<usize>) -> anyhow::Result<Vec<u8>> {
        use rust_xlsxwriter::Workbook;

        let snapshots = self.store.load_snapshots(url_or_id, limit)?;
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        for (col, header) in ["timestamp", "url", "http_status", "content"].iter().enumerate() {
            worksheet.write(0, col as u16, *header)?;
        }
        let mut row = 1u32;
        for s in snapshots {
            worksheet.write(row, 0, s.timestamp.to_rfc3339())?;
            worksheet.write(row, 1, s.url.as_str())?;
            worksheet.write(row, 2, s.http_status.map(|v| v.to_string()).unwrap_or_default())?;
            worksheet.write(row, 3, s.content.as_str())?;
            row += 1;
        }
        Ok(workbook.save_to_buffer()?)
    }

    pub async fn compare_urls(&self, url_a: &str, url_b: &str, diff_mode: DiffMode, target: Option<&str>) -> anyhow::Result<DiffReport> {
        let mut cfg_a    = WatchConfig::new(url_a);
        cfg_a.target     = target.map(|s| s.to_string());
        let mut cfg_b    = WatchConfig::new(url_b);
        cfg_b.target     = target.map(|s| s.to_string());

        let res_a    = fetcher::fetch_once(&cfg_a).await?;
        let res_b    = fetcher::fetch_once(&cfg_b).await?;

        let content_a    = cleaner::extract(&res_a.body, target, &[])?;
        let content_b    = cleaner::extract(&res_b.body, target, &[])?;

        let changes = diff::diff(&content_a, &content_b, diff_mode);
        Ok(DiffReport {
            url: format!("{url_a} <-> {url_b}"),
            label: "URL comparison".to_string(),
            timestamp: Utc::now(),
            changes,
            response_time_ms: None,
            http_status: None,
            ai_summary: None,
        })
    }

    pub async fn check_once(&self, url_or_id: &str) -> anyhow::Result<Option<DiffReport>> {
        let entry = {
            let watchers = self.watchers.lock().unwrap();
            watchers
                .values()
                .find(|e| e.config.key() == url_or_id || e.config.url == url_or_id)
                .cloned()
        };
        let Some(entry) = entry else { return Ok(None) };
        run_check(&entry, &self.store, &self.global_on_change, &self.json_store_dir, &self.browser_pool).await
    }

    pub fn start(&self) -> impl FnOnce() {
        self.running.store(true, Ordering::SeqCst);
        let watchers: Vec<Arc<WatcherEntry>>    = self.watchers.lock().unwrap().values().cloned().collect();
        let mut handles                         = Vec::new();

        for entry in watchers {
            let store          = self.store.clone();
            let global_cb      = self.global_on_change.clone();
            let running        = self.running.clone();
            let semaphore      = self.semaphore.clone();
            let storage_dir    = self.json_store_dir.clone();
            let browser_pool   = self.browser_pool.clone();

            let handle = tokio::spawn(async move {
                let mut first = true;
                loop {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }

                    if !first || entry.config.schedule.is_some() {
                        let wait_secs = next_wait_seconds(&entry);
                        tokio::time::sleep(std::time::Duration::from_secs_f64(wait_secs)).await;
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    first = false;

                    let paused = entry.runtime.lock().unwrap().paused;
                    if paused {
                        continue;
                    }
                    if in_maintenance_window(&entry.config, Utc::now()) {
                        continue;
                    }
                    if !in_active_hours(&entry.config, Utc::now()) {
                        continue;
                    }

                    let _permit = match &semaphore {
                        Some(s) => Some(s.clone().acquire_owned().await.unwrap()),
                        None => None,
                    };

                    if let Err(e) = run_check(&entry, &store, &global_cb, &storage_dir, &browser_pool).await {
                        tracing::warn!("check failed for {}: {e}", entry.config.url);
                    }
                }
            });
            handles.push(handle);
        }

        let extras: Vec<Arc<dyn ExtraWatcher>> = self.extra_watchers.lock().unwrap().clone();
        for extra in extras {
            let store        = self.store.clone();
            let running      = self.running.clone();
            let semaphore    = self.semaphore.clone();

            let handle = tokio::spawn(async move {
                let mut first = true;
                loop {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                    if !first {
                        tokio::time::sleep(std::time::Duration::from_secs_f64(extra.interval().max(1) as f64)).await;
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    first = false;
                    if extra.paused() {
                        continue;
                    }
                    let _permit = match &semaphore {
                        Some(s) => Some(s.clone().acquire_owned().await.unwrap()),
                        None => None,
                    };
                    extra.check(&store).await;
                }
            });
            handles.push(handle);
        }

        *self.tasks.lock().unwrap()    = handles;
        let running                    = self.running.clone();
        move || {
            running.store(false, Ordering::SeqCst);
        }
    }

    /// Starts the status HTTP server. Binds to 127.0.0.1 by default (matching
    /// the TS library's localhost-only default); pass `host` to override.
    pub async fn start_status_server(&self, port: u16, host: Option<&str>) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        let watchers = self.watchers.clone();
        let status_fn: Arc<dyn Fn() -> Vec<WatcherStatus> + Send + Sync> = Arc::new(move || {
            let watchers = watchers.lock().unwrap();
            watchers
                .values()
                .map(|entry| {
                    let rt = entry.runtime.lock().unwrap();
                    WatcherStatus {
                        url: entry.config.url.clone(),
                        id: entry.config.id.clone(),
                        label: entry.config.effective_label(),
                        target: entry.config.target.clone(),
                        interval: entry.config.interval,
                        paused: rt.paused,
                        in_maintenance: in_maintenance_window(&entry.config, Utc::now()),
                        last_check_at: rt.last_check_at,
                        next_check_at: rt.next_check_at,
                        last_change_at: rt.last_change_at,
                        checks_count: rt.checks_count,
                        changes_count: rt.changes_count,
                        errors_count: rt.errors_count,
                    }
                })
                .collect()
        });
        crate::status_server::start(port, host.unwrap_or("127.0.0.1"), status_fn).await
    }
}

fn next_wait_seconds(entry: &Arc<WatcherEntry>) -> f64 {
    if let Some(expr) = &entry.config.schedule {
        if let Ok(schedule) = expr.parse::<cron::Schedule>() {
            if let Some(next) = schedule.upcoming(Utc).next() {
                let secs = (next - Utc::now()).num_milliseconds() as f64 / 1000.0;
                return secs.max(0.1);
            }
        }
    }

    let base      = entry.config.interval as f64;
    let jitter    = entry.config.jitter.clamp(0.0, 1.0);
    if jitter > 0.0 {
        let factor = 1.0 + (rand::random::<f64>() * 2.0 - 1.0) * jitter;
        (base * factor).max(0.1)
    } else {
        base.max(0.1)
    }
}

fn in_maintenance_window(config: &WatchConfig, now: chrono::DateTime<Utc>) -> bool {
    config.maintenance_windows.iter().any(|w| now >= w.from && now <= w.to)
}

fn in_active_hours(config: &WatchConfig, now: chrono::DateTime<Utc>) -> bool {
    let Some(ab) = &config.active_between else { return true };

    let tz: chrono_tz::Tz = ab
        .timezone
        .as_deref()
        .and_then(|t| t.parse().ok())
        .unwrap_or(chrono_tz::UTC);
    let local = now.with_timezone(&tz);

    if !ab.days.is_empty() {
        let day_name = local.weekday().to_string().to_lowercase();
        if !ab.days.iter().any(|d| d.to_lowercase() == day_name) {
            return false;
        }
    }

    let parse_hm = |s: &str| -> Option<(u32, u32)> {
        let mut parts    = s.split(':');
        let h            = parts.next()?.parse().ok()?;
        let m            = parts.next().unwrap_or("0").parse().ok()?;
        Some((h, m))
    };
    let (fh, fm)    = parse_hm(&ab.from).unwrap_or((0, 0));
    let (th, tm)    = parse_hm(&ab.to).unwrap_or((23, 59));

    let now_minutes     = local.hour() * 60 + local.minute();
    let from_minutes    = fh * 60 + fm;
    let to_minutes      = th * 60 + tm;

    if from_minutes <= to_minutes {
        now_minutes >= from_minutes && now_minutes <= to_minutes
    } else {
        now_minutes >= from_minutes || now_minutes <= to_minutes
    }
}

async fn run_check(
    entry: &Arc<WatcherEntry>,
    store: &Arc<dyn Store>,
    global_cb: &Arc<Mutex<Vec<ChangeCallback>>>,
    storage_dir: &str,
    browser_pool: &Arc<crate::browser::BrowserPool>,
) -> anyhow::Result<Option<DiffReport>> {
    let config    = &entry.config;
    let key       = config.key();

    // `fetch_config` carries any headers an `onBeforeCheck` hook injects for
    // this one tick (e.g. a freshly refreshed bearer token) without mutating
    // the shared, immutable `entry.config`.
    let mut fetch_config_owned: Option<WatchConfig> = None;
    if let Some(hook) = &entry.handlers.on_before_check {
        match hook(config).await {
            Ok(extra_headers) if !extra_headers.is_empty() => {
                let mut patched = config.clone();
                patched.headers.extend(extra_headers);
                fetch_config_owned = Some(patched);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("onBeforeCheck hook failed for {}: {e}", config.url),
        }
    }
    let fetch_config = fetch_config_owned.as_ref().unwrap_or(config);

    let fetch_outcome = if config.browser {
        crate::browser::render_with_retry(browser_pool, fetch_config).await
    } else {
        fetcher::fetch_with_retry(fetch_config).await
    };

    let fetch_result = match fetch_outcome {
        Ok(r) => r,
        Err(e) => {
            let mut rt = entry.runtime.lock().unwrap();
            rt.errors_count += 1;
            rt.consecutive_failures += 1;
            rt.consecutive_successes = 0;
            let should_alert_failure = match &config.failure_policy {
                Some(fp) => fp.consecutive_failures.map(|n| rt.consecutive_failures >= n).unwrap_or(true),
                None => true,
            };
            if should_alert_failure {
                // Mark this watcher as needing a consecutive-success streak
                // before `on_recovery` fires (see `maybe_handle_recovery`).
                rt.alerts_suppressed = true;
            }
            drop(rt);
            if should_alert_failure {
                if let Some(cb) = &entry.handlers.on_error {
                    cb(anyhow::anyhow!("{e}"), config.clone());
                } else {
                    tracing::error!("fetch error for {}: {e}", config.url);
                }
            }
            return Err(e);
        }
    };

    if let Some(expected) = config.expected_status {
        if fetch_result.status != expected {
            if let Some(cb) = &entry.handlers.on_error {
                cb(
                    anyhow::anyhow!("expected status {expected}, got {}", fetch_result.status),
                    config.clone(),
                );
            }
        }
    }

    let extracted    = cleaner::extract(&fetch_result.body, config.target.as_deref(), &config.ignore_selectors)?;
    let processed    = cleaner::apply_ignores(&extracted, &config.ignore_patterns, config.ignore_numbers);

    let content = if let Some(path) = &config.json_path {
        cleaner::json_path_extract(&fetch_result.body, path).unwrap_or(processed)
    } else {
        processed
    };

    let previous            = store.load_snapshots(&key, Some(1))?;
    let previous_content    = previous.last().map(|s| s.content.clone());
    let previous_status = {
        let rt = entry.runtime.lock().unwrap();
        rt.last_http_status
    };

    let now = Utc::now();
    let snapshot = Snapshot {
        url: config.url.clone(),
        content: content.clone(),
        timestamp: now,
        http_status: Some(fetch_result.status),
    };

    if !config.dry_run {
        store.save_snapshot(&key, &snapshot)?;
        if let Some(max) = config.max_snapshots {
            store.prune_snapshots(&key, max)?;
        }
        if let Some(days) = config.retention_days {
            store.prune_older_than(&key, days)?;
        }
    }

    {
        let mut rt = entry.runtime.lock().unwrap();
        rt.checks_count += 1;
        rt.last_check_at           = Some(now);
        rt.consecutive_failures    = 0;
    }

    if let Some(threshold) = config.alert_if_no_change_after {
        let mut rt = entry.runtime.lock().unwrap();
        if let Some(last_change) = rt.last_change_at {
            let since = (now - last_change).num_seconds().max(0) as u64;
            if since >= threshold {
                let should_fire = rt
                    .last_silence_alert_at
                    .map(|last| (now - last).num_seconds() as u64 >= threshold)
                    .unwrap_or(true);
                if should_fire {
                    rt.last_silence_alert_at = Some(now);
                    drop(rt);
                    if let Some(cb) = &entry.handlers.on_silence {
                        cb(SilenceInfo {
                            url: config.url.clone(),
                            label: config.effective_label(),
                            seconds_since_last_change: since,
                        });
                    }
                }
            }
        }
    }

    if config.alert_on_status_change {
        if let Some(prev) = previous_status {
            if prev != fetch_result.status {
                if let Some(cb) = &entry.handlers.on_status_change {
                    cb(StatusChangeInfo {
                        url: config.url.clone(),
                        label: config.effective_label(),
                        previous_status: prev,
                        current_status: fetch_result.status,
                    });
                }
            }
        }
        entry.runtime.lock().unwrap().last_http_status = Some(fetch_result.status);
    } else {
        entry.runtime.lock().unwrap().last_http_status = Some(fetch_result.status);
    }

    let is_first_check = previous_content.is_none();

    let changes = match &previous_content {
        None => Vec::new(),
        Some(prev) => diff::diff(prev, &content, config.diff_mode),
    };

    let should_report = (!changes.is_empty() && changes.len() as u32 >= config.min_changes)
        || (is_first_check && config.alert_on_first_check);

    if !should_report {
        maybe_handle_recovery(entry, config);
        return Ok(None);
    }

    if config.change_threshold > 0.0 {
        if let Some(prev) = &previous_content {
            let frac = changed_fraction(prev, &content);
            if frac < config.change_threshold {
                maybe_handle_recovery(entry, config);
                return Ok(None);
            }
        }
    }

    // Flapping detection: re-verify the change is still present after a
    // delay before alerting, to suppress transient/flapping content.
    if let (Some(confirm_secs), Some(prev)) = (config.confirm_after, &previous_content) {
        tokio::time::sleep(std::time::Duration::from_secs(confirm_secs)).await;
        let reconfirm = fetcher::fetch_once(fetch_config).await;
        if let Ok(reconfirm) = reconfirm {
            let re_extracted = cleaner::extract(&reconfirm.body, config.target.as_deref(), &config.ignore_selectors)
                .unwrap_or_default();
            let re_processed = cleaner::apply_ignores(&re_extracted, &config.ignore_patterns, config.ignore_numbers);
            let re_content = if let Some(path) = &config.json_path {
                cleaner::json_path_extract(&reconfirm.body, path).unwrap_or(re_processed)
            } else {
                re_processed
            };
            let re_changes = diff::diff(prev, &re_content, config.diff_mode);
            if re_changes.is_empty() {
                // The change reverted before we could confirm it — treat as
                // flapping noise and suppress the alert.
                return Ok(None);
            }
        }
    }

    let mut report = DiffReport {
        url: config.url.clone(),
        label: config.effective_label(),
        timestamp: now,
        changes,
        response_time_ms: if config.track_response_time { Some(fetch_result.response_time_ms) } else { None },
        http_status: Some(fetch_result.status),
        ai_summary: None,
    };

    if config.ai_summary {
        let env_var = match config.ai_provider {
            AiProviderKind::Anthropic => "ANTHROPIC_API_KEY",
            AiProviderKind::Openai => "OPENAI_API_KEY",
            AiProviderKind::Gemini => "GEMINI_API_KEY",
        };
        let api_key = config.ai_api_key.clone().or_else(|| std::env::var(env_var).ok());
        if let Some(api_key) = api_key {
            use crate::ai::AiSummaryProvider;
            let provider: Box<dyn AiSummaryProvider> = match config.ai_provider {
                AiProviderKind::Anthropic => Box::new(crate::ai::AnthropicProvider::new(api_key)),
                AiProviderKind::Openai => Box::new(crate::ai::OpenAiProvider::new(api_key)),
                AiProviderKind::Gemini => Box::new(crate::ai::GeminiProvider::new(api_key)),
            };
            let dynamic_prompt = entry.handlers.ai_prompt_fn.as_ref().map(|f| f(&report));
            let prompt = dynamic_prompt.as_deref().or(config.ai_prompt.as_deref());
            match provider.summarize(&report, prompt).await {
                Ok(summary) => report.ai_summary = Some(summary),
                Err(e) => tracing::warn!("AI summary failed for {}: {e}", config.url),
            }
        } else {
            tracing::warn!("aiSummary enabled for {} but no API key (set aiApiKey or {env_var})", config.url);
        }
    }

    if !config.dry_run {
        store.save_report(&key, &report)?;

        if config.archive_html {
            let archive_dir = std::path::Path::new(storage_dir).join("archive");
            let filename = format!(
                "{}_{}.html",
                crate::store::hash_key(&key),
                now.format("%Y%m%dT%H%M%S%.3f")
            );
            if let Err(e) = std::fs::create_dir_all(&archive_dir)
                .and_then(|_| std::fs::write(archive_dir.join(filename), &fetch_result.body))
            {
                tracing::warn!("failed to archive HTML for {}: {e}", config.url);
            }
        }

        if config.screenshot_on_change {
            if let Some(png) = &fetch_result.screenshot {
                let archive_dir = std::path::Path::new(storage_dir).join("archive");
                let filename = format!(
                    "{}_{}.png",
                    crate::store::hash_key(&key),
                    now.format("%Y%m%dT%H%M%S%.3f")
                );
                if let Err(e) = std::fs::create_dir_all(&archive_dir)
                    .and_then(|_| std::fs::write(archive_dir.join(filename), png))
                {
                    tracing::warn!("failed to save screenshot for {}: {e}", config.url);
                }
            }
        }
    }

    {
        let mut rt = entry.runtime.lock().unwrap();
        rt.changes_count += 1;
        rt.last_change_at = Some(now);
        rt.change_timestamps.push(now);
        let window = config.change_spike_window.unwrap_or(0);
        if window > 0 {
            let cutoff = now - chrono::Duration::seconds(window as i64);
            rt.change_timestamps.retain(|t| *t >= cutoff);
        }
    }

    maybe_handle_recovery(entry, config);
    check_spike(entry, config, now);

    let alert_if_ok = entry.handlers.alert_if.as_ref().map(|f| f(&report)).unwrap_or(true);

    let cooldown_ok = alert_if_ok && {
        let mut rt = entry.runtime.lock().unwrap();
        let ok = match rt.last_alert_at {
            Some(last) => (now - last).num_seconds() as u64 >= config.cooldown,
            None => true,
        };
        if ok {
            rt.last_alert_at = Some(now);
        }
        ok
    };

    if !config.dry_run && cooldown_ok {
        for cb in &entry.handlers.on_change {
            cb(report.clone());
        }
        for cb in global_cb.lock().unwrap().iter() {
            cb(report.clone());
        }
        if !config.webhooks.is_empty() {
            webhook::send_all(&config.webhooks, &report, &config.webhook_headers, config.webhook_retries.unwrap_or(3)).await;
        }
        if let Some(email) = &config.email {
            if let Err(e) = crate::email::send_email(email, &report).await {
                tracing::warn!("email alert failed: {e}");
            }
        }
    } else if config.dry_run {
        for cb in &entry.handlers.on_change {
            cb(report.clone());
        }
    }

    Ok(Some(report))
}

fn maybe_handle_recovery(entry: &Arc<WatcherEntry>, config: &WatchConfig) {
    let mut rt = entry.runtime.lock().unwrap();
    rt.consecutive_successes += 1;
    if let Some(fp) = &config.failure_policy {
        if let Some(n) = fp.consecutive_successes {
            if rt.alerts_suppressed && rt.consecutive_successes >= n {
                rt.alerts_suppressed = false;
                drop(rt);
                if let Some(cb) = &entry.handlers.on_recovery {
                    cb(config.clone());
                }
            }
        }
    }
}

fn check_spike(entry: &Arc<WatcherEntry>, config: &WatchConfig, now: chrono::DateTime<Utc>) {
    let (Some(window), Some(threshold)) = (config.change_spike_window, config.change_spike_threshold) else {
        return;
    };
    let mut rt    = entry.runtime.lock().unwrap();
    let count     = rt.change_timestamps.len() as u32;
    if count >= threshold {
        let should_fire = match rt.last_spike_alert_at {
            Some(last) => (now - last).num_seconds() as u64 >= window,
            None => true,
        };
        if should_fire {
            rt.last_spike_alert_at = Some(now);
            drop(rt);
            if let Some(cb) = &entry.handlers.on_spike {
                cb(SpikeInfo {
                    url: config.url.clone(),
                    label: config.effective_label(),
                    changes_in_window: count,
                    window_seconds: window,
                });
            }
        }
    }
}

fn changed_fraction(before: &str, after: &str) -> f64 {
    let b_len                   = before.len().max(1);
    let diff                    = diff::diff(before, after, DiffMode::Line);
    let changed_chars: usize    = diff
        .iter()
        .map(|c| c.before.as_deref().unwrap_or("").len() + c.after.as_deref().unwrap_or("").len())
        .sum();
    (changed_chars as f64 / b_len as f64).min(1.0)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use std::sync::atomic::AtomicBool as StdAtomicBool;

    fn make_entry(failure_policy: FailurePolicy) -> Arc<WatcherEntry> {
        let mut config = WatchConfig::new("https://example.com");
        config.failure_policy = Some(failure_policy);
        Arc::new(WatcherEntry {
            config,
            handlers: Handlers::default(),
            runtime: Mutex::new(RuntimeState::default()),
        })
    }

    #[test]
    fn on_recovery_fires_only_after_a_prior_alerted_failure_and_enough_successes() {
        let entry = make_entry(FailurePolicy {
            consecutive_failures: Some(1),
            consecutive_successes: Some(2),
        });

        let fired = Arc::new(StdAtomicBool::new(false));
        let fired_clone = fired.clone();
        // SAFETY-free: just mutate Handlers directly since the field is pub(crate)-visible here.
        let entry = Arc::new(WatcherEntry {
            config: entry.config.clone(),
            handlers: Handlers {
                on_recovery: Some(Arc::new(move |_cfg| fired_clone.store(true, Ordering::SeqCst))),
                ..Default::default()
            },
            runtime: Mutex::new(RuntimeState::default()),
        });

        // No prior failure: successes alone must never fire on_recovery.
        maybe_handle_recovery(&entry, &entry.config);
        maybe_handle_recovery(&entry, &entry.config);
        assert!(!fired.load(Ordering::SeqCst), "on_recovery fired without a prior alerted failure");

        // Simulate the failure path flipping alerts_suppressed, as run_check does.
        entry.runtime.lock().unwrap().consecutive_failures = 1;
        entry.runtime.lock().unwrap().alerts_suppressed = true;
        entry.runtime.lock().unwrap().consecutive_successes = 0;

        maybe_handle_recovery(&entry, &entry.config); // 1st success after failure: not enough yet
        assert!(!fired.load(Ordering::SeqCst));

        maybe_handle_recovery(&entry, &entry.config); // 2nd success: threshold met
        assert!(fired.load(Ordering::SeqCst), "on_recovery should have fired after reaching consecutive_successes");
        assert!(!entry.runtime.lock().unwrap().alerts_suppressed);
    }
}
