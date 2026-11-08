use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Column, Row as SqlxRow};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DbDiffMode {
    #[default]
    Row,
    Schema,
    Aggregate,
    Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row(pub BTreeMap<String, Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DbChange {
    Inserted { row: Row },
    Deleted { row: Row },
    Updated { row_key: String, modifications: Vec<Modification> },
    SchemaChanged { changes: Vec<String> },
    ThresholdExceeded { before: f64, after: f64, change_percent: f64 },
    ValueChanged { before: String, after: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Modification {
    pub column: String,
    pub before: Value,
    pub after: Value,
}

/// A connection to one of the three supported database backends, picked
/// from the connection string's scheme (`sqlite://`, `postgresql://`/
/// `postgres://`, `mysql://`). SQLite uses `rusqlite` (sync, wrapped in a
/// `tokio::sync::Mutex` since `rusqlite::Connection` isn't `Sync`); Postgres
/// and MySQL use `sqlx` connection pools (native async).
pub enum DbConn {
    Sqlite(tokio::sync::Mutex<rusqlite::Connection>),
    Postgres(sqlx::PgPool),
    MySql(sqlx::MySqlPool),
}

pub async fn connect(connection_string: &str) -> anyhow::Result<DbConn> {
    if let Some(path) = connection_string.strip_prefix("sqlite://") {
        Ok(DbConn::Sqlite(tokio::sync::Mutex::new(rusqlite::Connection::open(path)?)))
    } else if connection_string.starts_with("postgresql://") || connection_string.starts_with("postgres://") {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(connection_string)
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect to PostgreSQL: {e}"))?;
        Ok(DbConn::Postgres(pool))
    } else if connection_string.starts_with("mysql://") {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(1)
            .connect(connection_string)
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect to MySQL: {e}"))?;
        Ok(DbConn::MySql(pool))
    } else {
        anyhow::bail!("unrecognized database connection string: {connection_string}")
    }
}

pub async fn fetch_rows(conn: &DbConn, query: &str) -> anyhow::Result<Vec<Row>> {
    match conn {
        DbConn::Sqlite(c) => fetch_rows_sqlite(&*c.lock().await, query),
        DbConn::Postgres(pool) => {
            let rows = sqlx::query(query).fetch_all(pool).await?;
            Ok(rows.iter().map(pg_row_to_row).collect())
        }
        DbConn::MySql(pool) => {
            let rows = sqlx::query(query).fetch_all(pool).await?;
            Ok(rows.iter().map(mysql_row_to_row).collect())
        }
    }
}

fn fetch_rows_sqlite(conn: &rusqlite::Connection, query: &str) -> anyhow::Result<Vec<Row>> {
    let mut stmt                  = conn.prepare(query)?;
    let col_names: Vec<String>    = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows_out              = Vec::new();
    let mut rows                  = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = BTreeMap::new();
        for (i, name) in col_names.iter().enumerate() {
            let value: Value = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                rusqlite::types::ValueRef::Real(f) => Value::from(f),
                rusqlite::types::ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                rusqlite::types::ValueRef::Blob(b) => Value::String(format!("<blob:{} bytes>", b.len())),
            };
            map.insert(name.clone(), value);
        }
        rows_out.push(Row(map));
    }
    Ok(rows_out)
}

/// Decodes a sqlx row into a JSON map without knowing column types ahead of
/// time, by trying common SQL type decodes in order. sqlx has no built-in
/// "any column -> serde_json::Value" decoder, since that's inherently
/// backend- and type-specific.
fn pg_row_to_row(row: &sqlx::postgres::PgRow) -> Row {
    let mut map = BTreeMap::new();
    for (i, col) in row.columns().iter().enumerate() {
        map.insert(col.name().to_string(), decode_pg_value(row, i));
    }
    Row(map)
}

fn decode_pg_value(row: &sqlx::postgres::PgRow, i: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    // NUMERIC/DECIMAL columns (e.g. prices) don't decode into f64 in sqlx —
    // they need rust_decimal::Decimal explicitly.
    if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(i) {
        return v.map(|d| Value::String(d.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.map(Value::String).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) {
        return v.map(|t| Value::String(t.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) {
        return v.map(|t| Value::String(t.to_rfc3339())).unwrap_or(Value::Null);
    }
    Value::String(format!("<undecoded:{}>", row.columns()[i].type_info()))
}

fn mysql_row_to_row(row: &sqlx::mysql::MySqlRow) -> Row {
    let mut map = BTreeMap::new();
    for (i, col) in row.columns().iter().enumerate() {
        map.insert(col.name().to_string(), decode_mysql_value(row, i));
    }
    Row(map)
}

fn decode_mysql_value(row: &sqlx::mysql::MySqlRow, i: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<u64>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return v.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(i) {
        return v.map(|d| Value::String(d.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.map(Value::String).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) {
        return v.map(|t| Value::String(t.to_string())).unwrap_or(Value::Null);
    }
    Value::String(format!("<undecoded:{}>", row.columns()[i].type_info()))
}

pub async fn fetch_schema(conn: &DbConn, table: &str) -> anyhow::Result<Vec<ColumnDef>> {
    match conn {
        DbConn::Sqlite(c) => fetch_schema_sqlite(&*c.lock().await, table),
        DbConn::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = $1",
            )
            .bind(table)
            .fetch_all(pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ColumnDef {
                    name: r.get::<String, _>(0),
                    col_type: r.get::<String, _>(1),
                    nullable: r.get::<String, _>(2) == "YES",
                })
                .collect())
        }
        DbConn::MySql(pool) => {
            let rows = sqlx::query(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = ?",
            )
            .bind(table)
            .fetch_all(pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ColumnDef {
                    name: r.get::<String, _>(0),
                    col_type: r.get::<String, _>(1),
                    nullable: r.get::<String, _>(2) == "YES",
                })
                .collect())
        }
    }
}

fn fetch_schema_sqlite(conn: &rusqlite::Connection, table: &str) -> anyhow::Result<Vec<ColumnDef>> {
    let mut stmt    = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows    = stmt.query([])?;
    let mut cols    = Vec::new();
    while let Some(row) = rows.next()? {
        let name: String        = row.get(1)?;
        let col_type: String    = row.get(2)?;
        let notnull: i64        = row.get(3)?;
        cols.push(ColumnDef {
            name,
            col_type,
            nullable: notnull == 0,
        });
    }
    Ok(cols)
}

pub async fn fetch_scalar(conn: &DbConn, query: &str) -> anyhow::Result<Value> {
    match conn {
        DbConn::Sqlite(c) => fetch_scalar_sqlite(&*c.lock().await, query),
        DbConn::Postgres(pool) => {
            let row = sqlx::query(query).fetch_optional(pool).await?;
            Ok(row.map(|r| decode_pg_value(&r, 0)).unwrap_or(Value::Null))
        }
        DbConn::MySql(pool) => {
            let row = sqlx::query(query).fetch_optional(pool).await?;
            Ok(row.map(|r| decode_mysql_value(&r, 0)).unwrap_or(Value::Null))
        }
    }
}

fn fetch_scalar_sqlite(conn: &rusqlite::Connection, query: &str) -> anyhow::Result<Value> {
    let mut stmt    = conn.prepare(query)?;
    let mut rows    = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let v: Value = match row.get_ref(0)? {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Integer(n) => Value::from(n),
            rusqlite::types::ValueRef::Real(f) => Value::from(f),
            rusqlite::types::ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
            rusqlite::types::ValueRef::Blob(b) => Value::String(format!("<blob:{} bytes>", b.len())),
        };
        Ok(v)
    } else {
        Ok(Value::Null)
    }
}

/// Ensures custom DB queries are read-only (SELECT-only) before execution.
pub fn validate_select_only(query: &str) -> anyhow::Result<()> {
    let trimmed = query.trim().to_lowercase();
    if !trimmed.starts_with("select") && !trimmed.starts_with("pragma") {
        anyhow::bail!("only SELECT (or PRAGMA) queries are allowed for DB monitoring: {query}");
    }
    let forbidden = ["insert", "update", "delete", "drop", "alter", "attach", "create", ";--"];
    for word in forbidden {
        if trimmed.contains(word) {
            anyhow::bail!("query contains a disallowed keyword '{word}': {query}");
        }
    }
    Ok(())
}

pub fn row_key(row: &Row, primary_key: &[String]) -> String {
    if primary_key.is_empty() {
        serde_json::to_string(&row.0).unwrap_or_default()
    } else {
        primary_key
            .iter()
            .map(|k| row.0.get(k).map(|v| v.to_string()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("|")
    }
}

pub fn diff_rows(
    before: &[Row],
    after: &[Row],
    primary_key: &[String],
    ignore_columns: &[String],
) -> Vec<DbChange> {
    use std::collections::HashMap;
    let mut changes = Vec::new();

    let before_map: HashMap<String, &Row>    = before.iter().map(|r| (row_key(r, primary_key), r)).collect();
    let after_map: HashMap<String, &Row>     = after.iter().map(|r| (row_key(r, primary_key), r)).collect();

    for (key, row) in &after_map {
        match before_map.get(key) {
            None => changes.push(DbChange::Inserted { row: (*row).clone() }),
            Some(old) => {
                let mut mods = Vec::new();
                for (col, new_val) in &row.0 {
                    if ignore_columns.contains(col) {
                        continue;
                    }
                    if let Some(old_val) = old.0.get(col) {
                        if old_val != new_val {
                            mods.push(Modification {
                                column: col.clone(),
                                before: old_val.clone(),
                                after: new_val.clone(),
                            });
                        }
                    }
                }
                if !mods.is_empty() {
                    changes.push(DbChange::Updated {
                        row_key: key.clone(),
                        modifications: mods,
                    });
                }
            }
        }
    }
    for (key, row) in &before_map {
        if !after_map.contains_key(key) {
            changes.push(DbChange::Deleted { row: (*row).clone() });
        }
    }
    changes
}

pub fn diff_schema(before: &[ColumnDef], after: &[ColumnDef]) -> Vec<String> {
    use std::collections::HashMap;
    let mut changes                              = Vec::new();
    let before_map: HashMap<&str, &ColumnDef>    = before.iter().map(|c| (c.name.as_str(), c)).collect();
    let after_map: HashMap<&str, &ColumnDef>     = after.iter().map(|c| (c.name.as_str(), c)).collect();

    for (name, col) in &after_map {
        match before_map.get(name) {
            None => changes.push(format!("column added: {name} ({})", col.col_type)),
            Some(old) => {
                if old.col_type != col.col_type {
                    changes.push(format!("column '{name}' type changed: {} -> {}", old.col_type, col.col_type));
                }
                if old.nullable != col.nullable {
                    changes.push(format!("column '{name}' nullability changed: {} -> {}", old.nullable, col.nullable));
                }
            }
        }
    }
    for name in before_map.keys() {
        if !after_map.contains_key(name) {
            changes.push(format!("column removed: {name}"));
        }
    }
    changes
}
