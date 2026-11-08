use crate::types::DiffReport;
use std::collections::HashMap;
use std::time::Duration;

pub async fn send_all(webhooks: &[String], report: &DiffReport, headers: &HashMap<String, String>, retries: u32) {
    let futures = webhooks.iter().map(|w| send_one(w, report, headers, retries));
    futures::future::join_all(futures).await;
}

async fn send_one(webhook: &str, report: &DiffReport, headers: &HashMap<String, String>, retries: u32) {
    let mut attempt    = 0;
    let mut delay      = 500u64;
    loop {
        match try_send(webhook, report, headers).await {
            Ok(_) => return,
            Err(e) => {
                if attempt >= retries {
                    tracing::warn!("webhook delivery failed for {webhook}: {e}");
                    return;
                }
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
                delay *= 2;
            }
        }
    }
}

async fn try_send(webhook: &str, report: &DiffReport, headers: &HashMap<String, String>) -> anyhow::Result<()> {
    let client     = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    let summary    = summarize(report);

    let (body, content_type_json) = build_payload(webhook, report, &summary);

    let mut req = client.post(webhook).timeout(Duration::from_secs(10));
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req = if content_type_json {
        req.json(&body)
    } else {
        req.header("Content-Type", "application/json").body(body.to_string())
    };

    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("webhook returned status {}", resp.status());
    }
    Ok(())
}

fn summarize(report: &DiffReport) -> String {
    let mut lines = vec![crate::helpers::report_summary(report)];
    if let Some(ai) = &report.ai_summary {
        lines.push(ai.clone());
    }
    for c in report.changes.iter().take(10) {
        lines.push(crate::helpers::change_human(c));
    }
    lines.join("\n")
}

fn build_payload(webhook: &str, report: &DiffReport, summary: &str) -> (serde_json::Value, bool) {
    if webhook.contains("discord.com") {
        let content: String = summary.chars().take(2000).collect();
        (serde_json::json!({ "content": content }), true)
    } else if webhook.contains("hooks.slack.com") {
        let text: String = summary.chars().take(3000).collect();
        (
            serde_json::json!({
                "text": text,
                "blocks": [
                    { "type": "header", "text": { "type": "plain_text", "text": report.label } },
                    { "type": "section", "text": { "type": "mrkdwn", "text": text } },
                    { "type": "context", "elements": [ { "type": "mrkdwn", "text": format!("<!date^{}^{{date_short_pretty}} {{time}}|{}>", report.timestamp.timestamp(), report.timestamp) } ] }
                ]
            }),
            true,
        )
    } else if webhook.contains("api.telegram.org") {
        (
            serde_json::json!({ "text": summary, "parse_mode": "HTML" }),
            true,
        )
    } else if webhook.contains("outlook.office.com") || webhook.contains("webhook.office.com") {
        (
            serde_json::json!({
                "@type": "MessageCard",
                "@context": "http://schema.org/extensions",
                "title": report.label,
                "text": summary
            }),
            true,
        )
    } else if webhook.contains("ntfy.sh") || webhook.contains("ntfy.") {
        (serde_json::json!({ "message": summary, "title": report.label }), true)
    } else {
        (serde_json::to_value(report).unwrap_or(serde_json::json!({})), true)
    }
}
