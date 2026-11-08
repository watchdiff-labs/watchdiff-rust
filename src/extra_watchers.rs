//! File, API-convenience, SSL certificate, sitemap, and database watchers.
//!
//! These share the same `Store` (keyed under distinct namespaces) as the web
//! pipeline in `watchdiff.rs` but run their own lightweight scheduling loop
//! since their fetch/diff steps differ from the HTML pipeline.

use crate::cert;
use crate::db::{self, DbChange, DbDiffMode};
use crate::fetcher;
use crate::sitemap;
use crate::store::Store;
use crate::types::DiffMode;
use crate::webhook;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait ExtraWatcher: Send + Sync {
    fn interval(&self) -> u64;
    fn paused(&self) -> bool;
    async fn check(&self, store: &Arc<dyn Store>);
}

// ---------- File monitoring ----------

#[derive(Default)]
pub struct FileWatchConfig {
    pub path: String,
    pub interval: u64,
    pub label: Option<String>,
    pub diff_mode: DiffMode,
    pub cooldown: u64,
    pub dry_run: bool,
    pub webhooks: Vec<String>,
    pub on_change: Option<Arc<dyn Fn(crate::types::DiffReport) + Send + Sync>>,
    pub on_error: Option<Arc<dyn Fn(anyhow::Error) + Send + Sync>>,
}

pub struct FileWatcher {
    cfg: FileWatchConfig,
    paused: AtomicBool,
    last_alert_at: Mutex<Option<chrono::DateTime<Utc>>>,
}

impl FileWatcher {
    pub fn new(cfg: FileWatchConfig) -> Self {
        Self {
            cfg,
            paused: AtomicBool::new(false),
            last_alert_at: Mutex::new(None),
        }
    }

    fn key(&self) -> String {
        format!("file:{}", self.cfg.path)
    }
}

#[async_trait]
impl ExtraWatcher for FileWatcher {
    fn interval(&self) -> u64 {
        self.cfg.interval
    }
    fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    async fn check(&self, store: &Arc<dyn Store>) {
        let content = match tokio::fs::read_to_string(&self.cfg.path).await {
            Ok(c) => c,
            Err(e) => {
                if let Some(cb) = &self.cfg.on_error {
                    cb(anyhow::anyhow!("{e}"));
                } else {
                    tracing::warn!("failed to read file '{}': {e}", self.cfg.path);
                }
                return;
            }
        };

        let key             = self.key();
        let previous        = store.load_snapshots(&key, Some(1)).unwrap_or_default();
        let prev_content    = previous.last().map(|s| s.content.clone());
        let now             = Utc::now();

        if !self.cfg.dry_run {
            let _ = store.save_snapshot(
                &key,
                &crate::types::Snapshot {
                    url: self.cfg.path.clone(),
                    content: content.clone(),
                    timestamp: now,
                    http_status: None,
                },
            );
        }

        let Some(prev)    = prev_content else { return };
        let changes       = crate::diff::diff(&prev, &content, self.cfg.diff_mode);
        if changes.is_empty() {
            return;
        }

        let report = crate::types::DiffReport {
            url: self.cfg.path.clone(),
            label: self.cfg.label.clone().unwrap_or_else(|| self.cfg.path.clone()),
            timestamp: now,
            changes,
            response_time_ms: None,
            http_status: None,
        ai_summary: None,
        };

        let cooldown_ok = {
            let mut last    = self.last_alert_at.lock().unwrap();
            let ok          = last.map(|l| (now - l).num_seconds() as u64 >= self.cfg.cooldown).unwrap_or(true);
            if ok {
                *last = Some(now);
            }
            ok
        };

        if let Some(cb) = &self.cfg.on_change {
            cb(report.clone());
        }
        if !self.cfg.dry_run && cooldown_ok && !self.cfg.webhooks.is_empty() {
            webhook::send_all(&self.cfg.webhooks, &report, &Default::default(), 3).await;
        }
    }
}

// ---------- SSL certificate monitoring ----------

#[derive(Default)]
pub struct CertWatchConfig {
    pub host: String,
    pub port: u16,
    pub interval: u64,
    pub label: Option<String>,
    pub warn_days_before_expiry: i64,
    pub webhooks: Vec<String>,
    pub on_change: Option<Arc<dyn Fn(cert::CertInfo) + Send + Sync>>,
    pub on_warning: Option<Arc<dyn Fn(cert::CertInfo) + Send + Sync>>,
    pub on_error: Option<Arc<dyn Fn(anyhow::Error) + Send + Sync>>,
}

pub struct CertWatcher {
    cfg: CertWatchConfig,
    paused: AtomicBool,
}

impl CertWatcher {
    pub fn new(cfg: CertWatchConfig) -> Self {
        Self { cfg, paused: AtomicBool::new(false) }
    }

    fn key(&self) -> String {
        format!("cert:{}:{}", self.cfg.host, self.cfg.port)
    }
}

#[async_trait]
impl ExtraWatcher for CertWatcher {
    fn interval(&self) -> u64 {
        self.cfg.interval
    }
    fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    async fn check(&self, store: &Arc<dyn Store>) {
        let info = match cert::fetch_cert(&self.cfg.host, self.cfg.port).await {
            Ok(i) => i,
            Err(e) => {
                if let Some(cb) = &self.cfg.on_error {
                    cb(e);
                } else {
                    tracing::warn!("cert fetch failed for {}: ", self.cfg.host);
                }
                return;
            }
        };

        let key            = self.key();
        let previous       = store.load_snapshots(&key, Some(1)).unwrap_or_default();
        let prev_serial    = previous.last().map(|s| s.content.clone());

        let now = Utc::now();
        let _ = store.save_snapshot(
            &key,
            &crate::types::Snapshot {
                url: format!("{}:{}", self.cfg.host, self.cfg.port),
                content: info.serial.clone(),
                timestamp: now,
                http_status: None,
            },
        );

        if let Some(prev) = prev_serial {
            if prev != info.serial {
                if let Some(cb) = &self.cfg.on_change {
                    cb(info.clone());
                }
            }
        }

        if info.days_until_expiry <= self.cfg.warn_days_before_expiry {
            if let Some(cb) = &self.cfg.on_warning {
                cb(info.clone());
            }
        }
    }
}

// ---------- Sitemap monitoring ----------

#[derive(Default)]
pub struct SitemapWatchConfig {
    pub url: String,
    pub interval: u64,
    pub label: Option<String>,
    pub webhooks: Vec<String>,
    pub on_change: Option<Arc<dyn Fn(Vec<String>, Vec<String>) + Send + Sync>>,
    pub on_error: Option<Arc<dyn Fn(anyhow::Error) + Send + Sync>>,
}

pub struct SitemapWatcher {
    cfg: SitemapWatchConfig,
    paused: AtomicBool,
}

impl SitemapWatcher {
    pub fn new(cfg: SitemapWatchConfig) -> Self {
        Self { cfg, paused: AtomicBool::new(false) }
    }

    fn key(&self) -> String {
        format!("sitemap:{}", self.cfg.url)
    }
}

#[async_trait]
impl ExtraWatcher for SitemapWatcher {
    fn interval(&self) -> u64 {
        self.cfg.interval
    }
    fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    async fn check(&self, store: &Arc<dyn Store>) {
        let cfg = crate::types::WatchConfig::new(self.cfg.url.clone());
        let fetch_result = match fetcher::fetch_once(&cfg).await {
            Ok(r) => r,
            Err(e) => {
                if let Some(cb) = &self.cfg.on_error {
                    cb(e);
                }
                return;
            }
        };

        let urls                          = sitemap::parse_sitemap_urls(&fetch_result.body);
        let key                           = self.key();
        let previous                      = store.load_snapshots(&key, Some(1)).unwrap_or_default();
        let prev_urls: HashSet<String>    = previous
            .last()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s.content).ok())
            .map(|v| v.into_iter().collect())
            .unwrap_or_default();

        let now           = Utc::now();
        let serialized    = serde_json::to_string(&urls.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
        let _ = store.save_snapshot(
            &key,
            &crate::types::Snapshot {
                url: self.cfg.url.clone(),
                content: serialized,
                timestamp: now,
                http_status: Some(fetch_result.status),
            },
        );

        if previous.is_empty() {
            return;
        }

        let d = sitemap::diff_sitemaps(&prev_urls, &urls);
        if d.added.is_empty() && d.removed.is_empty() {
            return;
        }

        if let Some(cb) = &self.cfg.on_change {
            cb(d.added.clone(), d.removed.clone());
        }

        if !self.cfg.webhooks.is_empty() {
            let report = crate::types::DiffReport {
                url: self.cfg.url.clone(),
                label: self.cfg.label.clone().unwrap_or_else(|| self.cfg.url.clone()),
                timestamp: now,
                changes: d
                    .added
                    .iter()
                    .map(|u| crate::types::Change { kind: "added".into(), before: None, after: Some(u.clone()), context: None })
                    .chain(d.removed.iter().map(|u| crate::types::Change {
                        kind: "removed".into(),
                        before: Some(u.clone()),
                        after: None,
                        context: None,
                    }))
                    .collect(),
                response_time_ms: None,
                http_status: Some(fetch_result.status),
            ai_summary: None,
            };
            webhook::send_all(&self.cfg.webhooks, &report, &Default::default(), 3).await;
        }
    }
}

// ---------- Database monitoring ----------

#[derive(Default)]
pub struct DbWatchConfig {
    pub connection_string: String,
    pub table: String,
    pub diff_mode: DbDiffMode,
    pub primary_key: Vec<String>,
    pub ignore_columns: Vec<String>,
    pub query: Option<String>,
    pub interval: u64,
    pub label: Option<String>,
    pub threshold: f64,
    pub webhooks: Vec<String>,
    pub on_change: Option<Arc<dyn Fn(Vec<DbChange>) + Send + Sync>>,
    pub on_schema_change: Option<Arc<dyn Fn(Vec<String>) + Send + Sync>>,
    pub on_threshold: Option<Arc<dyn Fn(f64, f64, f64) + Send + Sync>>,
    pub on_error: Option<Arc<dyn Fn(anyhow::Error) + Send + Sync>>,
}

pub struct DbWatcher {
    cfg: DbWatchConfig,
    paused: AtomicBool,
}

impl DbWatcher {
    pub fn new(cfg: DbWatchConfig) -> Self {
        Self { cfg, paused: AtomicBool::new(false) }
    }

    fn key(&self) -> String {
        format!("db:{}:{}:{:?}", self.cfg.connection_string, self.cfg.table, self.cfg.diff_mode)
    }
}

#[async_trait]
impl ExtraWatcher for DbWatcher {
    fn interval(&self) -> u64 {
        self.cfg.interval
    }
    fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    async fn check(&self, store: &Arc<dyn Store>) {
        let result = self.check_inner(store).await;
        if let Err(e) = result {
            if let Some(cb) = &self.cfg.on_error {
                cb(e);
            } else {
                tracing::warn!("db check failed for table '{}'", self.cfg.table);
            }
        }
    }
}

impl DbWatcher {
    async fn check_inner(&self, store: &Arc<dyn Store>) -> anyhow::Result<()> {
        let conn    = db::connect(&self.cfg.connection_string).await?;
        let key     = self.key();
        let now     = Utc::now();

        match self.cfg.diff_mode {
            DbDiffMode::Row => {
                let query = self.cfg.query.clone().unwrap_or_else(|| format!("SELECT * FROM {}", self.cfg.table));
                db::validate_select_only(&query)?;
                let rows          = db::fetch_rows(&conn, &query).await?;
                let serialized    = serde_json::to_string(&rows)?;

                let previous                   = store.load_snapshots(&key, Some(1))?;
                let prev_rows: Vec<db::Row>    = previous
                    .last()
                    .and_then(|s| serde_json::from_str(&s.content).ok())
                    .unwrap_or_default();

                store.save_snapshot(&key, &crate::types::Snapshot {
                    url: self.cfg.table.clone(),
                    content: serialized,
                    timestamp: now,
                    http_status: None,
                })?;

                if previous.is_empty() {
                    return Ok(());
                }

                let changes = db::diff_rows(&prev_rows, &rows, &self.cfg.primary_key, &self.cfg.ignore_columns);
                if !changes.is_empty() {
                    if let Some(cb) = &self.cfg.on_change {
                        cb(changes.clone());
                    }
                }
            }
            DbDiffMode::Schema => {
                let cols                             = db::fetch_schema(&conn, &self.cfg.table).await?;
                let serialized                       = serde_json::to_string(&cols)?;
                let previous                         = store.load_snapshots(&key, Some(1))?;
                let prev_cols: Vec<db::ColumnDef>    = previous
                    .last()
                    .and_then(|s| serde_json::from_str(&s.content).ok())
                    .unwrap_or_default();

                store.save_snapshot(&key, &crate::types::Snapshot {
                    url: self.cfg.table.clone(),
                    content: serialized,
                    timestamp: now,
                    http_status: None,
                })?;

                if previous.is_empty() {
                    return Ok(());
                }
                let changes = db::diff_schema(&prev_cols, &cols);
                if !changes.is_empty() {
                    if let Some(cb) = &self.cfg.on_schema_change {
                        cb(changes);
                    }
                }
            }
            DbDiffMode::Aggregate => {
                let query = self.cfg.query.clone().ok_or_else(|| anyhow::anyhow!("aggregate mode requires `query`"))?;
                db::validate_select_only(&query)?;
                let value    = db::fetch_scalar(&conn, &query).await?;
                let num      = value.as_f64().unwrap_or(0.0);

                let previous                 = store.load_snapshots(&key, Some(1))?;
                let prev_num: Option<f64>    = previous.last().and_then(|s| s.content.parse().ok());

                store.save_snapshot(&key, &crate::types::Snapshot {
                    url: self.cfg.table.clone(),
                    content: num.to_string(),
                    timestamp: now,
                    http_status: None,
                })?;

                if let Some(prev) = prev_num {
                    if prev != 0.0 {
                        let change_percent = ((num - prev) / prev).abs() * 100.0;
                        if change_percent >= self.cfg.threshold {
                            if let Some(cb) = &self.cfg.on_threshold {
                                cb(prev, num, change_percent);
                            }
                        }
                    }
                }
            }
            DbDiffMode::Value => {
                let query = self.cfg.query.clone().ok_or_else(|| anyhow::anyhow!("value mode requires `query`"))?;
                db::validate_select_only(&query)?;
                let value    = db::fetch_scalar(&conn, &query).await?;
                let text     = value.to_string();

                let previous     = store.load_snapshots(&key, Some(1))?;
                let prev_text    = previous.last().map(|s| s.content.clone());

                store.save_snapshot(&key, &crate::types::Snapshot {
                    url: self.cfg.table.clone(),
                    content: text.clone(),
                    timestamp: now,
                    http_status: None,
                })?;

                if let Some(prev) = prev_text {
                    if prev != text {
                        if let Some(cb) = &self.cfg.on_change {
                            cb(vec![DbChange::ValueChanged { before: prev, after: text }]);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
