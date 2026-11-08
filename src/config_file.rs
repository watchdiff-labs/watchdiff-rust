use crate::types::WatchConfig;
use scraper::Selector;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    #[serde(default = "default_storage_dir")]
    pub storage_dir: String,
    #[serde(default)]
    pub concurrency: Option<usize>,
    #[serde(default)]
    pub min_interval: Option<u64>,
    #[serde(default)]
    pub watchers: Vec<WatchConfig>,
}

fn default_storage_dir() -> String {
    ".watchdiff".to_string()
}

impl ConfigFile {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read config file '{}': {e}", path.as_ref().display()))?;
        let cfg: ConfigFile = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse config file '{}': {e}", path.as_ref().display()))?;
        Ok(cfg)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn example() -> Self {
        let mut watcher     = WatchConfig::new("https://example.com");
        watcher.label       = Some("Example page".to_string());
        watcher.interval    = 300;
        Self {
            storage_dir: default_storage_dir(),
            concurrency: None,
            min_interval: None,
            watchers: vec![watcher],
        }
    }

    /// Validates every watcher's config, returning all errors found rather
    /// than failing on the first one, so `watchdiff validate` can report
    /// everything in a single pass.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (i, w) in self.watchers.iter().enumerate() {
            let label = w.label.clone().unwrap_or_else(|| format!("watcher[{i}]"));

            if let Err(e) = crate::ssrf::validate_url(&w.url, w.allow_private_urls) {
                errors.push(format!("{label}: {e}"));
            }

            if let Some(target) = &w.target {
                if target.starts_with('/') || target.starts_with("./") {
                    if let Err(e) = libxml::xpath::Expression::compile(target) {
                        errors.push(format!("{label}: invalid XPath expression '{target}': {e:?}"));
                    }
                } else if Selector::parse(target).is_err() {
                    errors.push(format!("{label}: invalid CSS selector '{target}'"));
                }
            }

            if w.screenshot_on_change && !w.browser {
                errors.push(format!("{label}: screenshotOnChange requires browser: true"));
            }

            if let Some(schedule) = &w.schedule {
                if schedule.parse::<cron::Schedule>().is_err() {
                    errors.push(format!("{label}: invalid cron expression '{schedule}'"));
                }
            }

            if !(0.0..=1.0).contains(&w.change_threshold) {
                errors.push(format!("{label}: changeThreshold must be between 0 and 1"));
            }
            if !(0.0..=1.0).contains(&w.jitter) {
                errors.push(format!("{label}: jitter must be between 0 and 1"));
            }
            if w.interval == 0 && w.schedule.is_none() {
                errors.push(format!("{label}: interval must be > 0"));
            }
        }
        errors
    }
}
