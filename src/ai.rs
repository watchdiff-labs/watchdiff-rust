use crate::types::DiffReport;
use async_trait::async_trait;

/// A natural-language summary provider for `aiSummary`. Implemented for
/// Anthropic's Claude, OpenAI's Chat Completions, and Google's Gemini APIs —
/// select one via `WatchConfig.ai_provider`.
#[async_trait]
pub trait AiSummaryProvider: Send + Sync {
    async fn summarize(&self, report: &DiffReport, prompt_override: Option<&str>) -> anyhow::Result<String>;
}

fn default_prompt(report: &DiffReport) -> String {
    let diff_text = report
        .changes
        .iter()
        .map(|c| match (&c.before, &c.after) {
            (None, Some(a)) => format!("Added: {a}"),
            (Some(b), None) => format!("Removed: {b}"),
            (Some(b), Some(a)) => format!("Changed: \"{b}\" -> \"{a}\""),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Summarize this diff for a page titled '{}' in one or two sentences:\n{diff_text}",
        report.label
    )
}

fn prompt_for(report: &DiffReport, prompt_override: Option<&str>) -> String {
    prompt_override.map(|p| p.to_string()).unwrap_or_else(|| default_prompt(report))
}

pub struct AnthropicProvider {
    pub api_key: String,
    pub model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: "claude-sonnet-5".to_string() }
    }
}

#[async_trait]
impl AiSummaryProvider for AnthropicProvider {
    async fn summarize(&self, report: &DiffReport, prompt_override: Option<&str>) -> anyhow::Result<String> {
        let prompt = prompt_for(report, prompt_override);
        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": self.model,
                "max_tokens": 300,
                "messages": [{ "role": "user", "content": prompt }]
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Anthropic API error: {} - {}", resp.status(), resp.text().await.unwrap_or_default());
        }

        let body: serde_json::Value = resp.json().await?;
        Ok(body["content"][0]["text"].as_str().unwrap_or_default().to_string())
    }
}

pub struct OpenAiProvider {
    pub api_key: String,
    pub model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: "gpt-4o-mini".to_string() }
    }
}

#[async_trait]
impl AiSummaryProvider for OpenAiProvider {
    async fn summarize(&self, report: &DiffReport, prompt_override: Option<&str>) -> anyhow::Result<String> {
        let prompt = prompt_for(report, prompt_override);
        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "model": self.model,
                "max_tokens": 300,
                "messages": [{ "role": "user", "content": prompt }]
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OpenAI API error: {} - {}", resp.status(), resp.text().await.unwrap_or_default());
        }

        let body: serde_json::Value = resp.json().await?;
        Ok(body["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string())
    }
}

pub struct GeminiProvider {
    pub api_key: String,
    pub model: String,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: "gemini-2.0-flash".to_string() }
    }
}

#[async_trait]
impl AiSummaryProvider for GeminiProvider {
    async fn summarize(&self, report: &DiffReport, prompt_override: Option<&str>) -> anyhow::Result<String> {
        let prompt = prompt_for(report, prompt_override);
        let client = reqwest::Client::new();
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );
        let resp = client
            .post(&url)
            .json(&serde_json::json!({
                "contents": [{ "parts": [{ "text": prompt }] }]
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Gemini API error: {} - {}", resp.status(), resp.text().await.unwrap_or_default());
        }

        let body: serde_json::Value = resp.json().await?;
        Ok(body["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or_default().to_string())
    }
}
