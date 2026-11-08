use crate::store::Store;
use crate::types::{DiffReport, Snapshot};
use rusqlite::{params, Connection};
use std::sync::Mutex;

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn new(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL,
                content TEXT NOT NULL,
                http_status INTEGER,
                url TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_key ON snapshots(key);
            CREATE TABLE IF NOT EXISTS reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL,
                data TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reports_key ON reports(key);",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

impl Store for SqliteStore {
    fn save_snapshot(&self, key: &str, snapshot: &Snapshot) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO snapshots (key, content, http_status, url, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                key,
                snapshot.content,
                snapshot.http_status,
                snapshot.url,
                snapshot.timestamp.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    fn load_snapshots(&self, key: &str, limit: Option<usize>) -> anyhow::Result<Vec<Snapshot>> {
        let conn     = self.conn.lock().unwrap();
        let limit    = limit.unwrap_or(usize::MAX);
        let mut stmt = conn.prepare(
            "SELECT url, content, http_status, timestamp FROM snapshots WHERE key = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![key, limit as i64], |row| {
            let ts: String = row.get(3)?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(Snapshot {
                url: row.get(0)?,
                content: row.get(1)?,
                http_status: row.get(2)?,
                timestamp,
            })
        })?;
        let mut items: Vec<Snapshot> = rows.filter_map(|r| r.ok()).collect();
        items.reverse();
        Ok(items)
    }

    fn save_report(&self, key: &str, report: &DiffReport) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO reports (key, data, timestamp) VALUES (?1, ?2, ?3)",
            params![key, serde_json::to_string(report)?, report.timestamp.to_rfc3339()],
        )?;
        Ok(())
    }

    fn load_reports(&self, key: &str, limit: Option<usize>) -> anyhow::Result<Vec<DiffReport>> {
        let conn        = self.conn.lock().unwrap();
        let limit       = limit.unwrap_or(usize::MAX);
        let mut stmt    = conn.prepare("SELECT data FROM reports WHERE key = ?1 ORDER BY id DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![key, limit as i64], |row| {
            let data: String = row.get(0)?;
            Ok(data)
        })?;
        let mut items: Vec<DiffReport> = rows
            .filter_map(|r| r.ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();
        items.reverse();
        Ok(items)
    }

    fn clear(&self, key: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM snapshots WHERE key = ?1", params![key])?;
        conn.execute("DELETE FROM reports WHERE key = ?1", params![key])?;
        Ok(())
    }

    fn prune_snapshots(&self, key: &str, max_snapshots: usize) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM snapshots WHERE key = ?1 AND id NOT IN (
                SELECT id FROM snapshots WHERE key = ?1 ORDER BY id DESC LIMIT ?2
            )",
            params![key, max_snapshots as i64],
        )?;
        Ok(())
    }

    fn prune_older_than(&self, key: &str, retention_days: u32) -> anyhow::Result<()> {
        let cutoff    = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let conn      = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM snapshots WHERE key = ?1 AND timestamp < ?2",
            params![key, cutoff.to_rfc3339()],
        )?;
        Ok(())
    }
}
