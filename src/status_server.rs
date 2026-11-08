use crate::types::WatcherStatus;
use axum::{extract::State, routing::get, Json, Router};
use std::sync::Arc;
use tokio::task::JoinHandle;

type StatusFn = Arc<dyn Fn() -> Vec<WatcherStatus> + Send + Sync>;

#[derive(Clone)]
struct AppState {
    status_fn: StatusFn,
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn status(State(state): State<AppState>) -> Json<Vec<WatcherStatus>> {
    Json((state.status_fn)())
}

async fn metrics(State(state): State<AppState>) -> String {
    let statuses    = (state.status_fn)();
    let mut out     = String::new();
    out.push_str("# HELP watchdiff_checks_total Total checks performed\n# TYPE watchdiff_checks_total counter\n");
    for s in &statuses {
        out.push_str(&format!(
            "watchdiff_checks_total{{url=\"{}\"}} {}\n",
            escape(&s.url),
            s.checks_count
        ));
    }
    out.push_str("# HELP watchdiff_changes_total Total changes detected\n# TYPE watchdiff_changes_total counter\n");
    for s in &statuses {
        out.push_str(&format!(
            "watchdiff_changes_total{{url=\"{}\"}} {}\n",
            escape(&s.url),
            s.changes_count
        ));
    }
    out.push_str("# HELP watchdiff_errors_total Total errors encountered\n# TYPE watchdiff_errors_total counter\n");
    for s in &statuses {
        out.push_str(&format!(
            "watchdiff_errors_total{{url=\"{}\"}} {}\n",
            escape(&s.url),
            s.errors_count
        ));
    }
    out.push_str("# HELP watchdiff_paused Whether a watcher is paused\n# TYPE watchdiff_paused gauge\n");
    for s in &statuses {
        out.push_str(&format!(
            "watchdiff_paused{{url=\"{}\"}} {}\n",
            escape(&s.url),
            if s.paused { 1 } else { 0 }
        ));
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub async fn start(
    port: u16,
    host: &str,
    status_fn: StatusFn,
) -> anyhow::Result<JoinHandle<()>> {
    let state    = AppState { status_fn };
    let app      = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/metrics", get(metrics))
        .with_state(state);

    let addr        = format!("{host}:{port}");
    let listener    = tokio::net::TcpListener::bind(&addr).await?;
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("status server error: {e}");
        }
    });
    Ok(handle)
}
