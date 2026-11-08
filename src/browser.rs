//! Headless-browser rendering for JS-heavy pages (`browser: true`), via
//! chromiumoxide (CDP, native tokio async). See README "JS-heavy pages".

use crate::fetcher::FetchResult;
use crate::ssrf::validate_url;
use crate::types::{WaitFor, WatchConfig};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::EventResponseReceived;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Lazily launches a single shared headless Chromium instance on first use
/// and keeps it running for the lifetime of the `WatchDiff` process — one
/// per `WatchDiff`, not one per check (launching Chromium per tick would be
/// far too slow for any reasonable polling interval).
pub struct BrowserPool {
    inner: Mutex<Option<Arc<Browser>>>,
    executable: Option<PathBuf>,
}

impl BrowserPool {
    pub fn new() -> Self {
        Self { inner: Mutex::new(None), executable: None }
    }

    /// Uses a specific Chromium executable (e.g. the path returned by
    /// `fetch_chromium()`) instead of relying on chromiumoxide's
    /// auto-detection of a system-installed browser.
    pub fn with_executable(path: PathBuf) -> Self {
        Self { inner: Mutex::new(None), executable: Some(path) }
    }

    async fn get(&self) -> anyhow::Result<Arc<Browser>> {
        let mut guard = self.inner.lock().await;
        if let Some(browser) = &*guard {
            return Ok(browser.clone());
        }

        let mut builder = BrowserConfig::builder().no_sandbox();
        if let Some(exe) = &self.executable {
            builder = builder.chrome_executable(exe);
        }
        let config = builder
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build headless Chromium config: {e}"))?;

        let (browser, mut handler) = Browser::launch(config).await.map_err(|e| {
            anyhow::anyhow!(
                "failed to launch headless Chromium: {e}. Install Chrome/Chromium \
                 (e.g. `apt install chromium` / `brew install chromium`), or call \
                 `browser::fetch_chromium()` once at startup to auto-download one."
            )
        })?;

        // chromiumoxide splits the browser handle (for sending commands) from
        // a Handler that must be polled continuously to process CDP events —
        // without this task, every command on `browser`/`Page` would hang.
        tokio::spawn(async move {
            while handler.next().await.is_some() {}
        });

        let browser = Arc::new(browser);
        *guard = Some(browser.clone());
        Ok(browser)
    }
}

impl Default for BrowserPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Downloads a known-good Chromium build into the platform cache directory
/// (via `chromiumoxide_fetcher`) and returns its executable path, for
/// environments with no system Chrome/Chromium installed. Call this once at
/// startup and point `CHROME` (or your own launch config) at the result —
/// not wired into `BrowserPool` automatically, since downloading ~150-300MB
/// on every fresh checkout/container is a cost the caller should opt into
/// explicitly rather than hit by surprise on the first check.
pub async fn fetch_chromium() -> anyhow::Result<std::path::PathBuf> {
    use chromiumoxide_fetcher::{BrowserFetcher, BrowserFetcherOptions};

    let options = BrowserFetcherOptions::default()
        .map_err(|e| anyhow::anyhow!("failed to build Chromium fetcher options: {e}"))?;
    let fetcher = BrowserFetcher::new(options);
    let installation = fetcher
        .fetch()
        .await
        .map_err(|e| anyhow::anyhow!("failed to download Chromium: {e}"))?;
    Ok(installation.executable_path)
}

async fn render_once(pool: &BrowserPool, config: &WatchConfig) -> anyhow::Result<FetchResult> {
    validate_url(&config.url, config.allow_private_urls)?;

    let browser = pool.get().await?;
    let start = Instant::now();

    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| anyhow::anyhow!("failed to open a browser page: {e}"))?;

    // `Page::wait_for_navigation_response` resolves against an *already
    // in-flight* navigation; raced against `goto` via `join!` it reliably
    // returns `Ok(None)` because no navigation has started yet at the
    // instant it's polled. Subscribing to the raw CDP event stream first
    // (so the listener is registered before the navigation begins) and
    // taking the first `Document`-type response is what actually captures
    // the main navigation's status code.
    let mut responses = page
        .event_listener::<EventResponseReceived>()
        .await
        .map_err(|e| anyhow::anyhow!("failed to subscribe to network events: {e}"))?;

    page.goto(&config.url)
        .await
        .map_err(|e| anyhow::anyhow!("failed to navigate to {}: {e}", config.url))?;

    let status = tokio::time::timeout(Duration::from_millis(config.timeout_ms), async {
        use chromiumoxide::cdp::browser_protocol::network::ResourceType;
        while let Some(event) = responses.next().await {
            if matches!(event.r#type, ResourceType::Document) {
                return Some(event.response.status as u16);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(0);

    if let Some(opts) = &config.browser_options {
        if let Some(selector) = &opts.wait_for_selector {
            let deadline = Instant::now() + Duration::from_millis(config.timeout_ms);
            while page.find_element(selector.as_str()).await.is_err() {
                if Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        if matches!(opts.wait_for, Some(WaitFor::NetworkIdle)) {
            // chromiumoxide has no native CDP network-idle detector; this is
            // a pragmatic fixed grace period, not true idle detection.
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    }

    let body = page
        .content()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read rendered page content: {e}"))?;

    let screenshot = if config.screenshot_on_change {
        page.screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .full_page(true)
                .build(),
        )
        .await
        .map_err(|e| {
            tracing::warn!("screenshot capture failed for {}: {e}", config.url);
            e
        })
        .ok()
    } else {
        None
    };

    let _ = page.close().await;

    Ok(FetchResult {
        body,
        status,
        response_time_ms: start.elapsed().as_millis() as u64,
        screenshot,
    })
}

/// Same retry/backoff contract as `fetcher::fetch_with_retry`.
pub async fn render_with_retry(pool: &BrowserPool, config: &WatchConfig) -> anyhow::Result<FetchResult> {
    let mut attempt = 0u32;
    let mut delay = config.retry_delay_ms;

    loop {
        match render_once(pool, config).await {
            Ok(result) => return Ok(result),
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
