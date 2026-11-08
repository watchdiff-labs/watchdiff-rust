use crate::types::{DiffReport, Snapshot};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub trait Store: Send + Sync {
    fn save_snapshot(&self, key: &str, snapshot: &Snapshot) -> anyhow::Result<()>;
    fn load_snapshots(&self, key: &str, limit: Option<usize>) -> anyhow::Result<Vec<Snapshot>>;
    fn save_report(&self, key: &str, report: &DiffReport) -> anyhow::Result<()>;
    fn load_reports(&self, key: &str, limit: Option<usize>) -> anyhow::Result<Vec<DiffReport>>;
    fn clear(&self, key: &str) -> anyhow::Result<()>;
    fn prune_snapshots(&self, key: &str, max_snapshots: usize) -> anyhow::Result<()>;
    fn prune_older_than(&self, key: &str, retention_days: u32) -> anyhow::Result<()>;
}

pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
}

pub struct JsonStore {
    dir: PathBuf,
}

impl JsonStore {
    pub fn new(storage_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = storage_dir.as_ref().to_path_buf();
        fs::create_dir_all(dir.join("snapshots"))?;
        fs::create_dir_all(dir.join("reports"))?;
        fs::create_dir_all(dir.join("archive"))?;
        Ok(Self { dir })
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.dir.join("archive")
    }

    fn snapshot_path(&self, key: &str) -> PathBuf {
        self.dir.join("snapshots").join(format!("{}.jsonl", hash_key(key)))
    }

    fn report_path(&self, key: &str) -> PathBuf {
        self.dir.join("reports").join(format!("{}.jsonl", hash_key(key)))
    }

    fn append_line<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", serde_json::to_string(value)?)?;
        Ok(())
    }

    fn read_lines<T: serde::de::DeserializeOwned>(path: &Path, limit: Option<usize>) -> anyhow::Result<Vec<T>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content              = fs::read_to_string(path)?;
        let mut items: Vec<T>    = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        if let Some(n) = limit {
            if items.len() > n {
                items = items.split_off(items.len() - n);
            }
        }
        Ok(items)
    }
}

impl Store for JsonStore {
    fn save_snapshot(&self, key: &str, snapshot: &Snapshot) -> anyhow::Result<()> {
        Self::append_line(&self.snapshot_path(key), snapshot)
    }

    fn load_snapshots(&self, key: &str, limit: Option<usize>) -> anyhow::Result<Vec<Snapshot>> {
        Self::read_lines(&self.snapshot_path(key), limit)
    }

    fn save_report(&self, key: &str, report: &DiffReport) -> anyhow::Result<()> {
        Self::append_line(&self.report_path(key), report)
    }

    fn load_reports(&self, key: &str, limit: Option<usize>) -> anyhow::Result<Vec<DiffReport>> {
        Self::read_lines(&self.report_path(key), limit)
    }

    fn clear(&self, key: &str) -> anyhow::Result<()> {
        let _    = fs::remove_file(self.snapshot_path(key));
        let _    = fs::remove_file(self.report_path(key));
        Ok(())
    }

    fn prune_snapshots(&self, key: &str, max_snapshots: usize) -> anyhow::Result<()> {
        let mut snapshots: Vec<Snapshot> = Self::read_lines(&self.snapshot_path(key), None)?;
        if snapshots.len() > max_snapshots {
            snapshots    = snapshots.split_off(snapshots.len() - max_snapshots);
            let path     = self.snapshot_path(key);
            fs::remove_file(&path).ok();
            for s in &snapshots {
                Self::append_line(&path, s)?;
            }
        }
        Ok(())
    }

    fn prune_older_than(&self, key: &str, retention_days: u32) -> anyhow::Result<()> {
        let cutoff                      = Utc::now() - chrono::Duration::days(retention_days as i64);
        let snapshots: Vec<Snapshot>    = Self::read_lines(&self.snapshot_path(key), None)?;
        let kept: Vec<Snapshot>         = snapshots.into_iter().filter(|s| s.timestamp >= cutoff).collect();
        let path                        = self.snapshot_path(key);
        fs::remove_file(&path).ok();
        for s in &kept {
            Self::append_line(&path, s)?;
        }
        Ok(())
    }
}
