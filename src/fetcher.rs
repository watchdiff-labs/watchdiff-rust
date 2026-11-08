use crate::ssrf::validate_url;
use crate::types::WatchConfig;
use rand::seq::SliceRandom;
use std::time::{Duration, Instant};

pub struct FetchResult {
    pub body: String,
    pub status: u16,
    pub response_time_ms: u64,
    /// Only set by `browser::render` when `screenshotOnChange` is enabled.
    pub screenshot: Option<Vec<u8>>,
}

const DEFAULT_MAX_RESPONSE_SIZE: u64    = 10 * 1024 * 1024; // 10 MB
const MAX_REDIRECTS: usize              = 10;

fn pick<'a>(items: &'a [String]) -> Option<&'a str> {
    let mut rng = rand::thread_rng();
    items.choose(&mut rng).map(|s| s.as_str())
}

fn build_client(config: &WatchConfig) -> anyhow::Result<reqwest::Client> {
    let allow_private = config.allow_private_urls;
    // `Policy::limited` only caps hop count — it does not re-validate each
    // redirect target. A server (or a compromised/malicious one) could 302
    // from a public URL to a private/loopback address otherwise, bypassing
    // the SSRF guard entirely after the first hop. `custom` lets us run
    // `validate_url` on every redirect target, not just the initial URL.
    let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error(std::io::Error::other("too many redirects"));
        }
        match validate_url(attempt.url().as_str(), allow_private) {
            Ok(_) => attempt.follow(),
            Err(e) => attempt.error(std::io::Error::other(e.to_string())),
        }
    });

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .redirect(redirect_policy);

    if let Some(proxy) = pick(&config.proxies) {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    }

    Ok(builder.build()?)
}

/// Performs a single fetch attempt (no retry). SSRF validation is applied to
/// the initial URL and, via a custom redirect policy, to every subsequent
/// redirect target — not just the first hop.
pub async fn fetch_once(config: &WatchConfig) -> anyhow::Result<FetchResult> {
    let url       = validate_url(&config.url, config.allow_private_urls)?;
    let client    = build_client(config)?;

    let method = if config.request_body.is_some() {
        reqwest::Method::POST
    } else {
        reqwest::Method::GET
    };

    let mut req = client.request(method, url);

    for (k, v) in &config.headers {
        req = req.header(k, v);
    }

    if let Some(ua) = pick(&config.user_agents) {
        req = req.header("User-Agent", ua);
    }

    if !config.cookies.is_empty() {
        let cookie_header = config
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencoding_encode(v)))
            .collect::<Vec<_>>()
            .join("; ");
        req = req.header("Cookie", cookie_header);
    }

    if let Some(auth) = &config.auth {
        req = req.basic_auth(&auth.user, Some(&auth.pass));
    }

    if let Some(body) = &config.request_body {
        req = req.header("Content-Type", "application/json").body(body.clone());
    }

    let start     = Instant::now();
    let resp      = req.send().await?;
    let status    = resp.status().as_u16();

    let max_size    = config.max_response_size.unwrap_or(DEFAULT_MAX_RESPONSE_SIZE);
    let bytes       = read_capped(resp, max_size).await?;
    let elapsed     = start.elapsed().as_millis() as u64;

    let body = String::from_utf8_lossy(&bytes).into_owned();

    Ok(FetchResult {
        body,
        status,
        response_time_ms: elapsed,
        screenshot: None,
    })
}

async fn read_capped(resp: reqwest::Response, max_size: u64) -> anyhow::Result<Vec<u8>> {
    use futures::StreamExt;
    let mut stream    = resp.bytes_stream();
    let mut buf       = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.extend_from_slice(&chunk);
        if buf.len() as u64 > max_size {
            anyhow::bail!("response exceeded maxResponseSize ({max_size} bytes)");
        }
    }
    Ok(buf)
}

fn urlencoding_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Fetch with retry + exponential backoff. 4xx (except 429) are not retried.
pub async fn fetch_with_retry(config: &WatchConfig) -> anyhow::Result<FetchResult> {
    let mut attempt    = 0u32;
    let mut delay      = config.retry_delay_ms;

    loop {
        match fetch_once(config).await {
            Ok(result) => {
                if result.status >= 400 && result.status != 429 && result.status < 500 {
                    return Ok(result); // not a network error; caller decides on expected_status
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= config.retries {
                    return Err(e);
                }
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
                delay = (delay * 2).min(config.max_retry_delay_ms.unwrap_or(u64::MAX));
            }
        }
    }
}
