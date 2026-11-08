# watchdiff-core (Rust)

[![Crates.io](https://img.shields.io/badge/crates.io-unpublished-lightgrey)](#)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange?logo=rust)](#)
[![License: BSD-3-Clause](https://img.shields.io/badge/license-BSD--3--Clause-blue)](LICENSE)

**A Rust port of [watchdiff-core](https://www.npmjs.com/package/watchdiff-core) — lightweight web/file/API/database/SSL change monitoring with diffs, alerts and a CLI.**

watchdiff watches web pages, local files, REST/JSON APIs, SQLite/PostgreSQL/MySQL
databases, sitemaps, and SSL certificates — then tells you exactly what changed, with
structured diffs and alerts over webhooks or email. Same pipeline and concepts as the
TypeScript library, reimplemented natively with `tokio` + `reqwest`.

- **Deterministic** — same input always produces the same output
- **Human-readable diffs** — structured `Change` records, not a wall of HTML
- **5 diff modes** — line, word, semantic, JSON, RSS/Atom
- **CSS selectors and XPath** — target any zone of a page (XPath via a dedicated HTML-tolerant engine, see [XPath selectors](#xpath-selectors))
- **JS-heavy pages** — optional headless Chromium rendering via `chromiumoxide` (`browser: true`), plus screenshot-on-change
- **File / API / DB / SSL cert / sitemap monitoring**, not just web pages
- **SQLite, PostgreSQL, and MySQL** table monitoring — row/schema/aggregate/value diff modes on all three
- **5 webhook channels** — Discord, Slack (Block Kit), Telegram, Microsoft Teams, ntfy.sh
- **Email alerts** over SMTP
- **Cron scheduling**, interval jitter, exponential backoff retries
- **Pause / resume** by id, URL, or tag; concurrency limiting; maintenance windows; active hours
- **Flapping detection**, change-spike detection, HTTP status-change monitoring, silence detection
- **Condition-based alerts** (`alertIf`) and an async **pre-fetch hook** (`onBeforeCheck`)
- **AI summaries** via Anthropic Claude, OpenAI, or Gemini
- **SSRF-guarded fetcher** — loopback/private ranges blocked by default
- **JSON-lines or SQLite storage**, CSV/JSON/XLSX export, HTML archiving on change
- **Built-in status server** — `/health`, `/status`, `/metrics` (Prometheus)
- **Full CLI** — `init`, `run`, `validate`, `status`, `list`, `diff`, `snapshot`, `compare`, `pause`, `resume`, `export`, `clean`
- **Zero external services** — everything stored locally as JSON or SQLite

## Also available for other platforms

This project is also available as:

- **npm**: [watchdiff-core](https://www.npmjs.com/package/watchdiff-core)
- **PyPI**: [watchdiff-core](https://pypi.org/project/watchdiff-core/)

Same pipeline, same concepts, same diff output across every port. More environments
are listed at [github.com/watchdiff-labs](https://github.com/watchdiff-labs).

## At a glance

| What you want | How |
|---|---|
| Monitor a URL for changes | `wd.watch(WatchConfig::new(url))?` + `wd.start()` |
| Target a specific element | `cfg.target = Some(".price".into())` (CSS) or `Some("//span[@class='p']".into())` (XPath, auto-detected) |
| Get notified on change | `Handlers.on_change` / `cfg.webhooks = vec!["https://discord.com/...".into()]` |
| Render JS-heavy pages | `cfg.browser = true` (headless Chromium via `chromiumoxide`) |
| Avoid notification spam | `cfg.cooldown = 3600` |
| Retry on fetch errors | `cfg.retries = 3; cfg.retry_delay_ms = 1000` |
| Randomise polling interval | `cfg.jitter = 0.2` (±20% variance) |
| Word-level diff | `cfg.diff_mode = DiffMode::Word` |
| Diff a JSON API endpoint | `cfg.diff_mode = DiffMode::Json` |
| Rotate proxies / UAs | `cfg.proxies = vec![...]; cfg.user_agents = vec![...]` |
| Diff at paragraph level | `cfg.diff_mode = DiffMode::Semantic` |
| Persist to SQLite | `WatchDiffOptions { store: Some(Arc::new(SqliteStore::new(".db")?)), .. }` |
| Export history | `wd.export_reports_csv(url, None)?` / `wd.export_reports_json(url, None)?` / `wd.export_reports_xlsx(url, None)?` |
| Pause a watcher | `wd.pause(url)` / `wd.resume(url)` |
| Check watcher state | `wd.status()` |
| Alert on page silence | `cfg.alert_if_no_change_after = Some(86400)` + `Handlers.on_silence` |
| Dry-run (no save/alert) | `cfg.dry_run = true` |
| Limit snapshot storage | `cfg.max_snapshots = Some(100)` |
| Ignore counters / dates | `cfg.ignore_numbers = true` |
| Require significant change | `cfg.change_threshold = 0.05` (5% of content) |
| Handle errors gracefully | `Handlers.on_error = Some(Arc::new(\|err, cfg\| ...))` |
| Monitor an RSS/Atom feed | `cfg.diff_mode = DiffMode::Rss` |
| Archive HTML on change | `cfg.archive_html = true` |
| Screenshot on change | `cfg.browser = true; cfg.screenshot_on_change = true` (requires `browser: true`) |
| Detect change spikes | `cfg.change_spike_window = Some(300); cfg.change_spike_threshold = Some(5)` |
| Monitor HTTP status | `cfg.alert_on_status_change = true` + `Handlers.on_status_change` |
| Monitor a DB table | `wd.watch_db(DbWatchConfig { connection_string: "sqlite://./app.db".into(), table: "products".into(), diff_mode: DbDiffMode::Row, .. })` — also `postgresql://`/`mysql://` |
| Monitor DB schema | `DbWatchConfig { diff_mode: DbDiffMode::Schema, .. }` |
| Monitor a DB aggregate | `DbWatchConfig { diff_mode: DbDiffMode::Aggregate, query: Some("SELECT COUNT(*) FROM orders".into()), threshold: 10.0, .. }` |
| Monitor a local file | `wd.watch_file(FileWatchConfig { path: "/etc/nginx/nginx.conf".into(), interval: 60, .. })` |
| Monitor a REST API | `wd.watch_api(WatchConfig::new("https://api.example.com/prices"))?` |
| Track API response time | `cfg.track_response_time = true` → `report.response_time_ms` |
| Alert only on condition | `Handlers.alert_if = Some(Arc::new(\|report\| report.changes.iter().any(\|c\| c.context.as_deref() == Some("price"))))` |
| Monitor SSL certificate | `wd.watch_cert(CertWatchConfig { host: "example.com".into(), warn_days_before_expiry: 14, .. })` |
| AI summary of changes | `cfg.ai_summary = true` (defaults to Anthropic Claude) |
| Use a different AI provider | `cfg.ai_provider = AiProviderKind::Openai` (or `Gemini`) |
| Customize the AI prompt | `cfg.ai_prompt = Some("...".into())` (string) or `Handlers.ai_prompt_fn = Some(Arc::new(\|report\| ...))` (per-report function) |
| Fire checks on a schedule | `cfg.schedule = Some("0 9 * * *".into())` (cron expression) |
| Confirm change before alerting | `cfg.confirm_after = Some(30)` (flapping detection) |
| Monitor a sitemap for URL changes | `wd.watch_sitemap(SitemapWatchConfig { url: "https://example.com/sitemap.xml".into(), .. })` |
| Watch one JSON field only | `cfg.json_path = Some("$.data.price".into())` |
| Monitor authenticated pages | `cfg.cookies = HashMap::from([("session".into(), "abc123".into())])` |
| Send email on change | `cfg.email = Some(EmailAlertConfig { to: "...".into(), smtp: SmtpConfig { .. } })` |
| Give a watcher a stable ID | `cfg.id = Some("my-watcher".into())` then `wd.pause("my-watcher")` / `wd.resume("my-watcher")` |
| Limit concurrent checks | `WatchDiffOptions { concurrency: Some(5), .. }` |
| Skip checks during scheduled downtime | `cfg.maintenance_windows = vec![MaintenanceWindow { from, to }]` |
| Restrict monitoring to business hours | `cfg.active_between = Some(ActiveBetween { from: "09:00".into(), to: "18:00".into(), days: vec!["monday".into(), "friday".into()], .. })` |
| Require 3 failures before alerting | `cfg.failure_policy = Some(FailurePolicy { consecutive_failures: Some(3), .. })` |
| Validate config before deploying | `watchdiff validate` or `watchdiff validate --config my.json` |
| Diff two URLs | `wd.compare_urls(&url_a, &url_b, DiffMode::Line, None).await?` or `watchdiff compare <urlA> <urlB>` |
| Expose status over HTTP | `wd.start_status_server(9090, None).await?` or `--status-port 9090` |
| CLI one-liner | `watchdiff run https://example.com --target .price --interval 60` |
| Multi-URL config file | `watchdiff init` then edit `watchdiff.config.json` |
| Show watcher status | `watchdiff status` |
| Show last diff | `watchdiff diff https://example.com` |
| React when a watcher recovers | `Handlers.on_recovery = Some(Arc::new(\|cfg\| println!("Recovered: {}", cfg.effective_label())))` |
| Cap backoff delay | `cfg.max_retry_delay_ms = Some(30000)` (max 30s between retries) |
| Alert on very first check | `cfg.alert_on_first_check = true` |
| Add auth headers to webhooks | `cfg.webhook_headers = HashMap::from([("Authorization".into(), "Bearer token".into())])` |
| Auto-prune old snapshots | `cfg.retention_days = Some(30)` |
| Export as JSON | `wd.export_reports_json(url, None)?` / `watchdiff export url --format json` |
| View latest snapshot | `watchdiff snapshot https://example.com` |
| Remove orphan files | `watchdiff clean --config watchdiff.config.json` |
| HTTP Basic Auth | `cfg.auth = Some(BasicAuth { user: "alice".into(), pass: "secret".into() })` |
| Async pre-fetch hook | `Handlers.on_before_check = Some(Arc::new(\|cfg\| Box::pin(async move { /* refresh token */ Ok(headers_map) })))` — returned headers are merged into that check's request |
| POST to an API | `cfg.request_body = Some(r#"{"query":"SELECT 1"}"#.into())` |
| Group watchers by tag | `cfg.tags = vec!["prod".into()]` then `wd.pause("prod")` |
| List configured watchers | `watchdiff list` or `watchdiff list --config my.json` |
| Floor check frequency | `cfg.min_interval = Some(30)` (per-watcher) or `WatchDiffOptions { min_interval: Some(30), .. }` (global) |
| Cap watcher count | `WatchDiffOptions { max_watchers: Some(100), .. }` |
| Allow internal URLs | `cfg.allow_private_urls = true` (disables the SSRF guard for private-network monitoring) |
| Cap response body size | `cfg.max_response_size = Some(5_242_880)` (5 MB; default 10 MB) |
| Bind status server to custom host | `wd.start_status_server(9090, Some("0.0.0.0")).await?` |

## Quick navigation

- [Also available for other platforms](#also-available-for-other-platforms)
- [Install](#install--build)
- [Quick start](#quick-start)
- [How it works](#how-it-works)
- [Scope — not ported](#scope--not-ported)
- [API reference](#api-reference)
- [Feature details](#feature-details)
  - [JS-heavy pages (Playwright-equivalent)](#js-heavy-pages-playwright-equivalent)
  - [Proxy rotation](#proxy-rotation)
  - [User-Agent rotation](#user-agent-rotation)
  - [XPath selectors](#xpath-selectors)
  - [Diff modes](#diff-modes)
  - [Alert cooldown](#alert-cooldown)
  - [Retry with exponential backoff](#retry-with-exponential-backoff)
  - [Interval jitter](#interval-jitter)
  - [Pause and resume](#pause-and-resume)
  - [Watcher status](#watcher-status)
  - [Silence detection](#silence-detection)
  - [Dry-run mode](#dry-run-mode)
  - [Snapshot pruning](#snapshot-pruning)
  - [Change threshold](#change-threshold)
  - [Ignore numbers](#ignore-numbers)
  - [Error callback](#error-callback)
  - [Webhook channels](#webhook-channels)
  - [HTML archiving](#html-archiving)
  - [Screenshot on change](#screenshot-on-change)
  - [Change spike detection](#change-spike-detection)
  - [HTTP status monitoring](#http-status-monitoring)
  - [Database monitoring](#database-monitoring)
  - [RSS / Atom monitoring](#rss--atom-monitoring)
  - [AI summaries](#ai-summaries)
  - [File monitoring](#file-monitoring)
  - [API monitoring](#api-monitoring)
  - [SSL certificate monitoring](#ssl-certificate-monitoring)
  - [Condition-based alerts](#condition-based-alerts---alertif)
  - [Cron scheduling](#cron-scheduling)
  - [Flapping detection](#flapping-detection)
  - [Sitemap monitoring](#sitemap-monitoring)
  - [JSON path targeting](#json-path-targeting)
  - [Cookie-based authentication](#cookie-based-authentication)
  - [Email alerts](#email-alerts)
  - [onRecovery callback](#onrecovery-callback)
  - [maxRetryDelay](#maxretrydelay)
  - [alertOnFirstCheck](#alertonfirstcheck)
  - [webhookHeaders](#webhookheaders)
  - [Slack Block Kit](#slack-block-kit)
  - [retentionDays](#retentiondays)
  - [JSON export](#json-export)
  - [Stable watcher ID](#stable-watcher-id)
  - [Concurrency limiting](#concurrency-limiting)
  - [Maintenance windows](#maintenance-windows)
  - [Active hours](#active-hours)
  - [Failure policy](#failure-policy)
  - [URL comparison](#url-comparison)
  - [Status server](#status-server)
  - [SQLite storage backend](#sqlite-storage-backend)
  - [CSV and XLSX export](#csv-and-xlsx-export)
  - [Config file](#config-file-watchdiffconfigjson)
  - [Environment variables](#environment-variables)
  - [Validate config](#validate-config)
  - [HTTP Basic Auth](#http-basic-auth)
  - [SSRF protection](#ssrf-protection)
  - [onBeforeCheck hook](#onbeforecheck-hook)
  - [POST request support](#post-request-support)
  - [Watcher tags](#watcher-tags)
  - [minInterval](#mininterval)
- [WatchConfig reference](#watchconfig-reference)
- [Types](#types)
- [Helper functions](#helper-functions)
- [CLI reference](#cli-reference)
- [Advanced usage](#advanced-usage)
- [Optional dependencies](#optional-dependencies)
- [Project layout](#project-layout)
- [Testing](#testing)
- [Contributing](#contributing)
- [License](#license)

## Install / build

```bash
git clone <this repo>
cd watchdiff-rust
cargo build --release
./target/release/watchdiff --help
```

Requires Rust 1.88+ (edition 2021; the floor is set by the `libxml` dependency used for
XPath support). SQLite is bundled via `rusqlite`'s `bundled` feature — nothing to
install for that. XPath support (via libxml2) does need system packages, though:

```bash
# Debian / Ubuntu
sudo apt install libxml2-dev libclang-dev pkg-config

# Fedora
sudo dnf install libxml2-devel clang-devel pkgconf-pkg-config

# Arch
sudo pacman -S libxml2 clang pkgconf

# macOS
brew install libxml2 llvm
```

These are needed to **build** the crate (`libclang` for `bindgen`, `libxml2-dev` +
`pkg-config` to link against libxml2). The resulting binary also links `libxml2`
**dynamically at runtime** — unlike SQLite, it is not bundled/statically compiled in
by default, so the target machine needs `libxml2`'s shared library installed (it's
present on most Linux distros and macOS already; minimal/musl/Alpine images may need
`apk add libxml2`). See [XPath selectors](#xpath-selectors) for a fully static-linking
option if you need a dependency-free binary.

**JS-heavy page rendering** (`browser: true`) is a *runtime-only* requirement — no
extra build packages needed, just a Chrome/Chromium binary on the machine that runs
`watchdiff` (auto-detected; not required at all unless you set `browser: true`
somewhere):

```bash
# Debian / Ubuntu
sudo apt install chromium

# Fedora
sudo dnf install chromium

# Arch
sudo pacman -S chromium

# macOS
brew install --cask google-chrome   # or chromium
```

No installed browser? Call `watchdiff_core::browser::fetch_chromium()` once at
startup to download a known-good build automatically — see
[JS-heavy pages](#js-heavy-pages-playwright-equivalent).

## Quick start

### Library

```rust
use watchdiff_core::{WatchDiff, WatchDiffOptions};
use watchdiff_core::types::WatchConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let wd = WatchDiff::new(WatchDiffOptions::default())?;

    let mut cfg = WatchConfig::new("https://example.com/product");
    cfg.target = Some(".price".into());
    cfg.interval = 60;
    cfg.webhooks = vec!["https://ntfy.sh/my-topic".into()];
    wd.watch(cfg)?;

    let stop = wd.start();
    tokio::signal::ctrl_c().await?;
    stop();
    Ok(())
}
```

### CLI

```bash
watchdiff init                                                      # writes watchdiff.config.json
watchdiff validate                                                  # checks the config for errors
watchdiff run                                                        # runs every watcher in the config
watchdiff run https://example.com --target .price --interval 60     # one-liner, no config file needed
watchdiff status --config watchdiff.config.json
watchdiff list
watchdiff diff https://example.com
watchdiff snapshot https://example.com
watchdiff compare https://a.example.com https://b.example.com
watchdiff pause my-watcher-id
watchdiff resume my-watcher-id
watchdiff export https://example.com --format csv
watchdiff clean
```

## How it works

Every web check runs through a fixed pipeline:

```
Fetcher -> Cleaner -> DiffEngine -> Store -> Notifier
```

1. **Fetcher** (`fetcher.rs`) — downloads the page via `reqwest`, with SSRF validation
   re-applied to **every redirect hop** (not just the initial URL — see [SSRF
   protection](#ssrf-protection) below), proxy/User-Agent rotation, cookies, HTTP Basic
   Auth, retry with exponential backoff, a 10-hop redirect cap, and a response-size cap.
2. **Cleaner** (`cleaner.rs`) — strips `<script>`/`<style>`/`<noscript>`, extracts the
   target CSS selector (or full page text), applies `ignorePatterns`/`ignoreNumbers`,
   and optionally extracts a `jsonPath` sub-value.
3. **DiffEngine** (`diff.rs`) — compares content in line, word, semantic, JSON, or RSS mode.
4. **Store** (`store.rs` / `sqlite_store.rs`) — persists snapshots and reports as
   JSON-lines files or in SQLite.
5. **Notifier** (`webhook.rs`, `email.rs`, `ai.rs`) — optionally generates an AI summary,
   then fires callbacks, webhooks, and/or an SMTP email on a detected (and alert-worthy) change.

File, API, SSL-certificate, sitemap, and database monitoring (`extra_watchers.rs`)
follow the same shape — fetch/read, diff against the last snapshot, store, notify —
with pipelines specific to each source instead of the HTML cleaner.

## Scope — not ported

Every feature of the TS library has a real implementation in this port — there is no
remaining "not implemented" list. A few of them are deliberate, documented
simplifications rather than feature-for-feature ports, called out where they apply:

- [JS-heavy pages](#js-heavy-pages-playwright-equivalent) — a from-scratch CDP
  integration (`chromiumoxide`), not a port of Playwright's auto-waiting heuristics;
  `networkidle` is approximated as a fixed grace period, and only Chromium is
  supported (no Firefox/WebKit).
- [XPath selectors](#xpath-selectors) — evaluated via libxml2, which is not
  guaranteed thread-safe across OS threads; every XPath evaluation is serialized
  behind a process-wide lock (CSS selectors stay fully concurrent).
- [Database monitoring](#database-monitoring) — PostgreSQL/MySQL column values are
  decoded by trying common SQL types in sequence (no sqlx `Any`-row decoder exists);
  an exotic column type not covered falls back to a `<undecoded:TYPE>` placeholder
  string rather than silently dropping the column.

## API reference

### `WatchDiff::new`

```rust
let wd = WatchDiff::new(WatchDiffOptions {
    storage_dir: ".watchdiff".into(),
    store: None,                 // or Some(Arc::new(SqliteStore::new(".watchdiff.db")?))
    concurrency: Some(10),       // cap simultaneous in-flight checks
    min_interval: Some(30),      // global floor on check interval, seconds
    max_watchers: Some(1000),
})?;
```

### `.watch(config)`

Registers a web watcher. Chainable (`&Self`). See [WatchConfig reference](#watchconfig-reference)
for every field.

```rust
let mut cfg = WatchConfig::new("https://example.com/product");
cfg.target = Some(".price".into());
cfg.diff_mode = DiffMode::Word;
cfg.interval = 60;
cfg.cooldown = 3600;
cfg.retries = 3;
wd.watch(cfg)?;
```

Use `.watch_with_handlers(config, handlers)` instead when you need callbacks
(`on_change`, `on_error`, `on_silence`, `on_spike`, `on_status_change`, `on_recovery`,
`alert_if`, `on_before_check`) — see [Callbacks](#feature-details).

### Extra watchers

```rust
wd.watch_file(FileWatchConfig { path: "/etc/nginx/nginx.conf".into(), interval: 60, .. });
wd.watch_api(WatchConfig::new("https://api.example.com/prices"))?;   // diffMode defaults to json
wd.watch_cert(CertWatchConfig { host: "example.com".into(), port: 443, warn_days_before_expiry: 14, .. });
wd.watch_sitemap(SitemapWatchConfig { url: "https://example.com/sitemap.xml".into(), .. });
wd.watch_db(DbWatchConfig { connection_string: "sqlite://./app.db".into(), table: "products".into(), .. });
```

All four config structs live in `extra_watchers.rs` and take an `interval`, optional
`label`, `webhooks`, and type-specific `on_change`/`on_error`-style callbacks.

### Callbacks

```rust
use watchdiff_core::Handlers;
use std::sync::Arc;

let handlers = Handlers {
    on_change: vec![Arc::new(|report| println!("{} changed: {:?}", report.label, report.changes))],
    on_error: Some(Arc::new(|err, cfg| eprintln!("{}: {err}", cfg.url))),
    // Alert only when a specific context changed; cooldown isn't consumed when this returns false.
    alert_if: Some(Arc::new(|report| report.changes.iter().any(|c| c.context.as_deref() == Some("price")))),
    // Async hook fired before every fetch tick. The returned headers are
    // merged into *this one check's* request only — e.g. a freshly
    // refreshed bearer token, without mutating the shared WatchConfig.
    on_before_check: Some(Arc::new(|cfg| {
        let url = cfg.url.clone();
        Box::pin(async move {
            println!("checking {url}");
            let mut headers = std::collections::HashMap::new();
            headers.insert("Authorization".into(), format!("Bearer {}", refresh_token().await?));
            Ok(headers)
        })
    })),
    // Compute the AI summary prompt per-report (overrides `cfg.ai_prompt`).
    ai_prompt_fn: Some(Arc::new(|report| format!("In French, summarize: {:?}", report.changes))),
    ..Default::default()
};
wd.watch_with_handlers(cfg, handlers)?;

// Or a global callback fired for every watcher:
wd.on_change(Arc::new(|report| { /* ... */ }));
```

### Scheduler controls

```rust
let stop = wd.start();     // spawns one tokio task per watcher, returns a stop closure
wd.pause("my-watcher-id"); // by id, URL, or tag
wd.resume("my-watcher-id");
let statuses = wd.status(); // Vec<WatcherStatus>
stop();
```

### One-off checks

```rust
let report = wd.check_once("https://example.com").await?;   // single immediate check
let report = wd.compare_urls(&url_a, &url_b, DiffMode::Word, Some(".price")).await?;
```

### History, reports & export

```rust
let snapshots = wd.history("https://example.com", Some(10))?;
let reports   = wd.reports("https://example.com", Some(20))?;
wd.clear("https://example.com")?;

let json = wd.export_reports_json("https://example.com", None)?;
let csv  = wd.export_reports_csv("https://example.com", None)?;
let csv2 = wd.export_snapshots_csv("https://example.com", None)?;
```

### Status server

```rust
wd.start_status_server(9090, None).await?;   // binds 127.0.0.1:9090 by default
wd.start_status_server(9090, Some("0.0.0.0")).await?;
```

## Feature details

### JS-heavy pages (Playwright-equivalent)

```rust
cfg.browser = true;
cfg.browser_options = Some(BrowserOptions {
    wait_for: Some(WaitFor::NetworkIdle),       // or Load / DomContentLoaded
    wait_for_selector: Some(".content".into()),
});
```

Renders the page in headless Chromium via [`chromiumoxide`](https://github.com/mattsse/chromiumoxide)
(native CDP, async, built for tokio) instead of a plain HTTP fetch, then feeds the
rendered HTML through the **exact same** cleaner/diff pipeline — CSS selectors and
XPath both work identically whether a page was browser-rendered or not.

```bash
watchdiff run https://spa-example.com --browser --interval 120
```

**Requirements**: a Chrome/Chromium binary. By default `BrowserPool` auto-detects one
already installed on the system (`google-chrome`, `chromium`, etc.). If none is found:

```rust
let chrome_path = watchdiff_core::browser::fetch_chromium().await?; // downloads ~150-300MB, cached after the first run
let wd = WatchDiff::new(WatchDiffOptions {
    browser_executable: Some(chrome_path),
    ..Default::default()
})?;
```

This is opt-in, not automatic — a fresh checkout or container never silently triggers
a large download just because a watcher has `browser: true`; you choose when to call
`fetch_chromium()` (e.g. once at startup, behind a flag).

**Known simplifications versus Playwright** (this is a from-scratch CDP integration,
not a port of Playwright's auto-waiting heuristics):

- `WaitFor::NetworkIdle` is a fixed ~750ms grace period after `load`, not true CDP
  network-idle detection (chromiumoxide has no built-in equivalent to Playwright's
  `waitUntil: "networkidle"`). `Load`/`DomContentLoaded` map directly to the browser's
  own load event via `Page::goto`, which already waits for it.
- `waitForSelector` polls `find_element` every 200ms up to `timeout` — a simple
  retry loop, not a native MutationObserver-backed wait.
- One headless Chromium instance is launched lazily on first use and shared by every
  `browser: true` watcher in a `WatchDiff` (launching a fresh browser per check would
  be far too slow for any reasonable polling interval) — so all browser-rendered
  checks across watchers share that one browser process (each still gets its own page/tab).
- No multi-browser support (Chromium only — no Firefox/WebKit, unlike Playwright).

HTTP status is captured from the first `Document`-type network response CDP reports
for the navigation (falls back to `0` if none arrives within `timeout`).

### Proxy rotation

```rust
cfg.proxies = vec![
    "http://user:pass@proxy1.example.com:8080".into(),
    "socks5://proxy3.example.com:1080".into(),
];
```

One proxy is picked at random per request (`fetcher.rs`), distributing load across the
pool to avoid IP-based rate-limiting.

### User-Agent rotation

```rust
cfg.user_agents = vec![
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/124.0.0.0".into(),
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Safari/604.1".into(),
];
```

### XPath selectors

```rust
cfg.target = Some("//div[@class='price']".into()); // auto-detected: starts with `/` or `./`
```

A `target` starting with `/` or `./` is evaluated as XPath via **libxml2** (through the
`libxml` crate) rather than CSS via `scraper`. This specifically uses libxml2's
HTML-parsing mode (`Parser::default_html()`), the same engine behind Python's
`lxml.html` — it tolerates real-world malformed markup (unclosed `<br>`, unquoted
attributes, missing `<html>`/`<body>`, etc.), not just strict well-formed XML.

```rust
wd.watch(WatchConfig {
    target: Some("//table[@id='results']//tr[position()>1]/td[2]".into()),
    label: Some("Table column 2".into()),
    ..WatchConfig::new("https://example.com")
})?;
```

Caveats specific to this path:

- **System dependency**: building requires `libxml2-dev` + `libclang` + `pkg-config`
  (see [Install](#install--build)); the binary links `libxml2` dynamically at
  runtime, unlike the bundled SQLite. A fully static binary is possible but requires
  supplying your own static `libxml2.a` via the `LIBXML2_STATIC` env var and
  `PKG_CONFIG_PATH` — see the [`libxml` crate's README](https://github.com/KWARC/rust-libxml#static-linking-musl--alpine--fully-static-binaries).
- **Serialized evaluation**: libxml2 is not guaranteed thread-safe for concurrent
  parsing/evaluation across OS threads. `watchdiff-rust` serializes every XPath
  evaluation behind a single process-wide lock (`cleaner::XPATH_LOCK`) to stay
  correct under `WatchDiff`'s multi-threaded scheduler. CSS-selector extraction
  (`scraper`, pure Rust) is unaffected and stays fully concurrent — so prefer CSS
  when you can, XPath when you need `position()`, axes, or other expressions CSS
  can't express.
- **stderr noise**: libxml2 prints its own structured parse/XPath diagnostics to
  stderr (e.g. `XPath error : Invalid predicate`) in addition to the `Result` error
  returned to your code — harmless, but visible in logs. `watchdiff validate`
  surfaces the same error through its normal error-reporting path.
- An invalid XPath expression returns a descriptive `anyhow::Error`; `watchdiff
  validate` compiles every XPath `target` up front (`Expression::compile`, no
  document needed) to catch typos before you deploy.

### Diff modes

| Mode | Best for | Granularity |
|---|---|---|
| `line` (default) | General text content | Line by line (Myers diff via `similar`) |
| `word` | Short values, prices, status text | Word by word |
| `semantic` | Blog posts, structured pages | Non-empty line/paragraph blocks |
| `json` | REST APIs, JSON feeds | Nested key path |
| `rss` | RSS 2.0 / Atom feeds | Item by item (`guid`/`id`) |

`json` and `rss` fall back to `line` diff automatically when the content can't be
parsed as JSON / doesn't contain `<item>`/`<entry>` elements.

### Alert cooldown

```rust
cfg.cooldown = 3600; // at most one alert per hour
```

A change detected within the cooldown window is still stored in history — only the
alert (callbacks, webhooks, email) is suppressed.

### Retry with exponential backoff

```rust
cfg.retries = 3;
cfg.retry_delay_ms = 2000;          // 2s -> 4s -> 8s between attempts
cfg.max_retry_delay_ms = Some(30000); // cap at 30s
```

4xx errors other than 429 are not retried; network errors, timeouts, 5xx, and 429 are.

### Interval jitter

```rust
cfg.jitter = 0.2; // actual interval randomised ±20% around `interval`
```

### Pause and resume

```rust
wd.pause("https://example.com");  // by URL, id, or tag
wd.resume("https://example.com");
```

The CLI equivalents (`watchdiff pause`/`resume <selector>`) persist the paused flag into
`watchdiff.config.json` since the CLI process exits between invocations.

### Watcher status

```rust
let statuses = wd.status();
for s in &statuses {
    println!("{}: {} checks, {} changes, paused={}", s.label, s.checks_count, s.changes_count, s.paused);
}
```

```bash
watchdiff status                # reads watchdiff.config.json
watchdiff status --json         # machine-readable
```

### Silence detection

```rust
cfg.alert_if_no_change_after = Some(86400); // 24 hours
```

```rust
let handlers = Handlers {
    on_silence: Some(Arc::new(|info| {
        eprintln!("{} has not changed in {}s", info.label, info.seconds_since_last_change);
    })),
    ..Default::default()
};
```

Fires at most once per `alertIfNoChangeAfter` seconds; the timer resets on the next
detected change.

### Dry-run mode

```rust
cfg.dry_run = true;
```

```bash
watchdiff run https://example.com --target .price --dry-run
```

Snapshots and reports are not saved, callbacks/webhooks/email are not fired — `on_change`
handlers still run so you can inspect what *would* have been alerted.

### Snapshot pruning

```rust
cfg.max_snapshots = Some(50); // keep the last 50 snapshots per watcher
```

Works with both `JsonStore` and `SqliteStore`; does not affect stored reports.

### Change threshold

```rust
cfg.change_threshold = 0.05; // only alert if at least 5% of content changed
```

Fraction from `0.0` (any change, default) to `1.0` (the entire content must change).
Changes below the threshold are still stored, just not alerted.

### Ignore numbers

```rust
cfg.ignore_numbers = true; // "1,234 views updated at 14:52" -> no change detected
```

### Error callback

```rust
let handlers = Handlers {
    on_error: Some(Arc::new(|err, cfg| {
        eprintln!("{}: {err}", cfg.url);
    })),
    ..Default::default()
};
```

### Webhook channels

Payload format is auto-detected from the URL. Each webhook POST times out after 10s
and failures don't block other webhooks (`futures::join_all`).

| URL pattern | Channel | Payload |
|---|---|---|
| `discord.com` | Discord | `{ content }` (≤ 2000 chars) |
| `hooks.slack.com` | Slack | Block Kit (header + section + context) |
| `api.telegram.org` | Telegram Bot API | `{ text, parse_mode: "HTML" }` |
| `outlook.office.com` / `webhook.office.com` | Microsoft Teams | MessageCard JSON |
| `ntfy.sh` / `ntfy.` | ntfy.sh | `{ message, title }` |
| anything else | Generic | Full `DiffReport` as JSON |

### HTML archiving

```rust
cfg.archive_html = true;
```

Saves the full raw HTML of the page to `<storageDir>/archive/<hash>_<timestamp>.html`
on every detected, alert-worthy change. Archived files are never pruned automatically.

### Screenshot on change

```rust
cfg.browser = true;             // required
cfg.screenshot_on_change = true;
```

Captures a full-page PNG via the headless Chromium session used for `browser: true`
on every detected, alert-worthy change, saved to
`<storageDir>/archive/<hash>_<timestamp>.png` — same naming scheme as
[HTML archiving](#html-archiving). `watchdiff validate` rejects
`screenshotOnChange: true` without `browser: true`. A failed capture logs a warning
and does not interrupt the rest of the check.

### Change spike detection

```rust
cfg.change_spike_window = Some(300);    // 5-minute window
cfg.change_spike_threshold = Some(5);   // alert if >= 5 changes in that window
```

```rust
let handlers = Handlers {
    on_spike: Some(Arc::new(|info| {
        eprintln!("{}: {} changes in {}s", info.label, info.changes_in_window, info.window_seconds);
    })),
    ..Default::default()
};
```

Fires at most once per `changeSpikeWindow` seconds to avoid repeated alerts during the
same spike.

### HTTP status monitoring

```rust
cfg.alert_on_status_change = true;
```

```rust
let handlers = Handlers {
    on_status_change: Some(Arc::new(|info| {
        println!("{}: {} -> {}", info.label, info.previous_status, info.current_status);
    })),
    ..Default::default()
};
```

`cfg.expected_status` additionally fires `on_error` when a check's status code differs
from the expected one. The `/metrics` status-server endpoint exposes
`watchdiff_checks_total`/`watchdiff_errors_total` per URL (no dedicated last-status
gauge — unlike the TS library's `watchdiff_last_http_status`).

### Database monitoring

`watch_db` supports three backends, picked from the connection string's scheme:

| Scheme | Backend | Driver |
|---|---|---|
| `sqlite://` | SQLite | `rusqlite` (sync, local file) |
| `postgresql://` / `postgres://` | PostgreSQL | `sqlx::PgPool` (native async) |
| `mysql://` | MySQL | `sqlx::MySqlPool` (native async) |

Four diff modes, identical across all three backends:

| Mode | Compares | Use case |
|---|---|---|
| `Row` | Rows by primary key | Inserts/deletes/column-level updates |
| `Schema` | Column defs (name, type, nullability) | Detect `ALTER TABLE`-style changes |
| `Aggregate` | A numeric scalar query result | Alert when a count changes by ≥ N% |
| `Value` | An arbitrary scalar query result | Watch a single config flag or cursor |

```rust
wd.watch_db(DbWatchConfig {
    connection_string: "postgresql://user:pass@localhost/mydb".into(), // or sqlite://, mysql://
    table: "products".into(),
    diff_mode: DbDiffMode::Row,
    primary_key: vec!["id".into()],
    ignore_columns: vec!["updated_at".into()],
    interval: 60,
    on_change: Some(Arc::new(|changes| println!("{changes:?}"))),
    ..Default::default()
});
```

Custom queries are validated as SELECT/PRAGMA-only (`db::validate_select_only`) before
execution — no INSERT/UPDATE/DELETE/DROP/ALTER/ATTACH/CREATE allowed.

**Column decoding for Postgres/MySQL**: sqlx has no generic "any column type ->
`serde_json::Value`" decoder, since that's inherently backend- and type-specific. Each
column is decoded by trying common SQL types in order — integers, floats, booleans,
`NUMERIC`/`DECIMAL` (via `rust_decimal`), text, then timestamps — falling back to a
`<undecoded:TYPE>` placeholder string for anything more exotic (arrays, JSON/JSONB
columns, custom enum types, etc.) rather than silently dropping the column or
panicking. SQLite's native dynamic typing doesn't have this issue.

Verified against real PostgreSQL 16 and MySQL 8 instances: row inserts/updates are
detected correctly, including `NUMERIC`/`DECIMAL` price-style columns.

### RSS / Atom monitoring

```rust
cfg.diff_mode = DiffMode::Rss;
```

Parses the feed XML, identifies items by `guid` (RSS) or `id` (Atom), and reports
added/removed/modified items individually instead of diffing raw XML. Falls back to
`line` diff if no `<item>`/`<entry>` elements are found.

### AI summaries

```rust
cfg.ai_summary = true;
cfg.ai_provider = AiProviderKind::Anthropic; // or Openai / Gemini — default is Anthropic
cfg.ai_api_key = Some("sk-ant-...".into());  // or set ANTHROPIC_API_KEY / OPENAI_API_KEY / GEMINI_API_KEY
cfg.ai_prompt = Some("Summarize this diff in French.".into());
```

```rust
// Or compute the prompt per-report (mirrors the TS library's
// `aiPrompt: (report) => string`) via Handlers, not WatchConfig — it takes
// precedence over cfg.ai_prompt when set.
let handlers = Handlers {
    ai_prompt_fn: Some(Arc::new(|report| format!("In French, summarize: {:?}", report.changes))),
    ..Default::default()
};
```

| Provider | Implementation | Default model | API key env var |
|---|---|---|---|
| `Anthropic` (default) | `ai::AnthropicProvider` | `claude-sonnet-5` | `ANTHROPIC_API_KEY` |
| `Openai` | `ai::OpenAiProvider` | `gpt-4o-mini` | `OPENAI_API_KEY` |
| `Gemini` | `ai::GeminiProvider` | `gemini-2.0-flash` | `GEMINI_API_KEY` |

All three implement the same `ai::AiSummaryProvider` trait (one async `summarize`
method), so adding another provider means implementing that trait, not touching the
scheduler. The generated summary is attached to `DiffReport.ai_summary` and prepended
to the webhook message body. A failure (missing key, API error) logs a warning and the
check continues without a summary — it never blocks the rest of the pipeline.

### File monitoring

```rust
wd.watch_file(FileWatchConfig {
    path: "/etc/nginx/nginx.conf".into(),
    interval: 60,
    diff_mode: DiffMode::Line,
    cooldown: 0,
    dry_run: false,
    webhooks: vec![],
    on_change: Some(Arc::new(|report| println!("{report:?}"))),
    on_error: None,
    label: None,
});
```

Runs through the same diff pipeline as web pages, minus HTTP-specific options
(headers, proxies, cookies).

### API monitoring

```rust
let mut cfg = WatchConfig::new("https://api.example.com/prices");
cfg.expected_status = Some(200);
wd.watch_api(cfg)?; // defaults diffMode to json and enables trackResponseTime
```

### SSL certificate monitoring

```rust
wd.watch_cert(CertWatchConfig {
    host: "example.com".into(),
    port: 443,
    interval: 3600,
    warn_days_before_expiry: 14,
    webhooks: vec![],
    on_change: Some(Arc::new(|info| println!("cert replaced: {info:?}"))),   // serial changed
    on_warning: Some(Arc::new(|info| println!("expires in {} days", info.days_until_expiry))),
    on_error: None,
    label: None,
});
```

`on_change`/`on_warning` receive a `cert::CertInfo`:

| Field | Type | Meaning |
|---|---|---|
| `subject` | `String` | Certificate subject (e.g. `CN=example.com`) |
| `serial` | `String` | Serial number — compared across checks to detect a cert replacement |
| `not_after` | `DateTime<Utc>` | Expiry timestamp |
| `days_until_expiry` | `i64` | `not_after - now`, in days (negative if already expired) |

### Condition-based alerts — `alertIf`

```rust
let handlers = Handlers {
    alert_if: Some(Arc::new(|report| report.changes.iter().any(|c| c.context.as_deref() == Some("price")))),
    ..Default::default()
};
```

When `alert_if` returns `false`, the change is still detected and stored, but no
callback/webhook/email fires and the cooldown timer is **not** consumed.

### Cron scheduling

```rust
cfg.schedule = Some("0 9 * * *".into()); // every day at 09:00 UTC, overrides `interval`
```

Standard 5-field cron expressions via the `cron` crate. `watchdiff validate` checks the
expression parses before you deploy it.

### Flapping detection

```rust
cfg.confirm_after = Some(30); // wait 30s, re-fetch, and re-diff before alerting
```

If the re-fetched content no longer differs from the previous snapshot, the alert is
suppressed as transient/flapping noise — nothing is stored or notified for that tick.

### Sitemap monitoring

```rust
wd.watch_sitemap(SitemapWatchConfig {
    url: "https://example.com/sitemap.xml".into(),
    interval: 3600,
    webhooks: vec![],
    on_change: Some(Arc::new(|added, removed| println!("+{} -{}", added.len(), removed.len()))),
    on_error: None,
    label: None,
});
```

### JSON path targeting

```rust
cfg.json_path = Some("$.data.price".into());
```

Extracts a sub-value from a JSON response body before diffing (e.g. `$.stock.available`,
`$.items[0].name`).

### Cookie-based authentication

```rust
cfg.cookies = HashMap::from([
    ("session".into(), "abc123".into()),
    ("csrftoken".into(), "xyz".into()),
]);
```

Cookie values are percent-encoded before being merged into the `Cookie` header.

### Email alerts

```rust
cfg.email = Some(EmailAlertConfig {
    to: "ops@example.com".into(),
    smtp: SmtpConfig { host: "smtp.example.com".into(), port: 587, user: "...".into(), pass: "...".into() },
});
```

Sent via `lettre` over STARTTLS/rustls — no external SMTP binary required.

### onRecovery callback

```rust
let handlers = Handlers {
    on_recovery: Some(Arc::new(|cfg| println!("Recovered: {}", cfg.effective_label()))),
    ..Default::default()
};
cfg.failure_policy = Some(FailurePolicy { consecutive_failures: Some(3), consecutive_successes: Some(2) });
```

Fires once a watcher has had `consecutiveSuccesses` successful checks in a row after
having been in a failing state.

### maxRetryDelay

```rust
cfg.max_retry_delay_ms = Some(30000); // cap exponential backoff at 30s
```

### alertOnFirstCheck

```rust
cfg.alert_on_first_check = true; // fire on_change on the very first snapshot capture
```

### webhookHeaders

```rust
cfg.webhook_headers = HashMap::from([("Authorization".into(), "Bearer token".into())]);
```

Merged into every outgoing webhook POST for that watcher.

### Slack Block Kit

Slack webhooks (`hooks.slack.com`) automatically get an enriched payload — header,
section, and context blocks — instead of a flat `text` field. See [Webhook channels](#webhook-channels).

### retentionDays

```rust
cfg.retention_days = Some(30); // auto-delete snapshots older than 30 days, after each save
```

### JSON export

```rust
let json = wd.export_reports_json("https://example.com", None)?;
let json_snaps = wd.export_snapshots_json("https://example.com", None)?;
```

```bash
watchdiff export https://example.com --format json
```

### Stable watcher ID

```rust
cfg.id = Some("my-watcher".into());
```

```rust
wd.pause("my-watcher");
wd.resume("my-watcher");
```

Lets multiple watchers share the same URL while remaining individually addressable, and
decouples storage keys from the URL.

### Concurrency limiting

```rust
let wd = WatchDiff::new(WatchDiffOptions { concurrency: Some(5), ..Default::default() })?;
```

A `tokio::sync::Semaphore` caps simultaneous in-flight checks across every watcher
(including file/cert/sitemap/DB watchers).

### Maintenance windows

```rust
cfg.maintenance_windows = vec![MaintenanceWindow {
    from: "2026-09-01T02:00:00Z".parse()?,
    to:   "2026-09-01T04:00:00Z".parse()?,
}];
```

Checks falling inside any window are skipped entirely — no fetch, no state change.

### Active hours

```rust
cfg.active_between = Some(ActiveBetween {
    from: "09:00".into(),
    to: "18:00".into(),
    days: vec!["monday".into(), "friday".into()],
    timezone: Some("America/New_York".into()), // IANA tz via chrono-tz; defaults to UTC
});
```

### Failure policy

```rust
cfg.failure_policy = Some(FailurePolicy {
    consecutive_failures: Some(3),  // require 3 failures in a row before firing on_error
    consecutive_successes: Some(2), // require 2 successes in a row before firing on_recovery
});
```

### URL comparison

```rust
let report = wd.compare_urls("https://a.example.com", "https://b.example.com", DiffMode::Word, Some(".price")).await?;
```

```bash
watchdiff compare https://a.example.com https://b.example.com --diff-mode word --target .price
```

No storage is written; it's a one-shot diff between two arbitrary URLs.

### Status server

```rust
wd.start_status_server(9090, None).await?;           // binds 127.0.0.1:9090 by default
wd.start_status_server(9090, Some("0.0.0.0")).await?; // override the bind host
```

```bash
watchdiff run --status-port 9090
```

Endpoints: `GET /health`, `GET /status` (JSON array of `WatcherStatus`), `GET /metrics`
(Prometheus text — `watchdiff_checks_total`, `watchdiff_changes_total`,
`watchdiff_errors_total`, `watchdiff_paused`, labeled by `url`).

### SQLite storage backend

```rust
use watchdiff_core::sqlite_store::SqliteStore;
use std::sync::Arc;

let wd = WatchDiff::new(WatchDiffOptions {
    store: Some(Arc::new(SqliteStore::new(".watchdiff.db")?)),
    ..Default::default()
})?;
```

An alternative to the default `JsonStore` for higher-volume workloads; same `Store`
trait, same snapshot/report semantics.

### CSV and XLSX export

```rust
let csv = wd.export_reports_csv("https://example.com", None)?;
std::fs::write("reports.csv", csv)?;

let xlsx: Vec<u8> = wd.export_reports_xlsx("https://example.com", None)?;
std::fs::write("reports.xlsx", xlsx)?;

// Every export_reports_* has a snapshots counterpart:
let snap_csv  = wd.export_snapshots_csv("https://example.com", None)?;
let snap_json = wd.export_snapshots_json("https://example.com", None)?;
let snap_xlsx: Vec<u8> = wd.export_snapshots_xlsx("https://example.com", None)?;
```

```bash
watchdiff export https://example.com --format csv
watchdiff export https://example.com --format csv --snapshots
watchdiff export https://example.com --format xlsx --output reports.xlsx
```

XLSX is binary (`rust_xlsxwriter`, pure Rust, no C dependency), so the CLI requires
`--output <path>` for that format — it can't be printed to stdout like JSON/CSV.
`--output` also works with `--format json`/`csv` to write to a file instead of stdout.

### Config file (`watchdiff.config.json`)

```json
{
  "storageDir": ".watchdiff",
  "concurrency": 10,
  "minInterval": 30,
  "watchers": [
    {
      "url": "https://example.com",
      "label": "Example page",
      "interval": 300,
      "diffMode": "line"
    }
  ]
}
```

`watchdiff init` generates this file. The CLI always goes through it, but the format
is also usable directly from the library via `config_file::ConfigFile`:

```rust
use watchdiff_core::config_file::ConfigFile;

let cfg_file = ConfigFile::load("watchdiff.config.json")?;
let errors = cfg_file.validate(); // same checks `watchdiff validate` runs
for watcher_config in cfg_file.watchers {
    wd.watch(watcher_config)?;
}
```

### Environment variables

| Variable | Used by |
|---|---|
| `ANTHROPIC_API_KEY` | AI summaries with `ai_provider: Anthropic` (the default), when `aiApiKey` isn't set on the watcher |
| `OPENAI_API_KEY` | AI summaries with `ai_provider: Openai` |
| `GEMINI_API_KEY` | AI summaries with `ai_provider: Gemini` |

### Validate config

```bash
watchdiff validate
watchdiff validate --config my.json
```

Checks every watcher for invalid URLs (including the SSRF guard, unless
`allowPrivateUrls`), invalid CSS selectors or XPath expressions (`target`), bad cron
expressions, and out-of-range `changeThreshold`/`jitter` values — before you deploy.

### HTTP Basic Auth

```rust
cfg.auth = Some(BasicAuth { user: "alice".into(), pass: "secret".into() });
```

Adds an `Authorization: Basic ...` header automatically.

### SSRF protection

```rust
cfg.allow_private_urls = true; // disables the guard below entirely, for internal-network monitoring
```

Every URL is checked against loopback, link-local, private (RFC 1918), CGNAT
(100.64.0.0/10), and the IPv6 equivalents (`::1`, `fc00::/7`, `fe80::/10`) before any
request — **on the initial URL and on every redirect hop**, via a custom
`reqwest::redirect::Policy` (not just `Policy::limited`, which caps hop count but
never re-validates where each hop actually points). Without this, a server could 302
from an allowed public URL to an internal address and bypass the guard after the first
hop.

**Known limitation**: this guard checks literal IP addresses in the URL; it does not
perform DNS resolution, so a hostname that only *resolves* to a private address (DNS
rebinding) is not caught — matching the scope of the equivalent check in watchdiff-ts.
`cfg.allow_private_urls = true` disables the guard entirely, for watchers that
intentionally target internal infrastructure.

### onBeforeCheck hook

```rust
let handlers = Handlers {
    on_before_check: Some(Arc::new(|cfg| {
        let url = cfg.url.clone();
        Box::pin(async move {
            println!("checking {url}");
            let token = refresh_token().await?; // your own async token refresh
            Ok(HashMap::from([("Authorization".to_string(), format!("Bearer {token}"))]))
        })
    })),
    ..Default::default()
};
```

The returned `HashMap<String, String>` is merged into the request headers for **that
one check only** — `WatchConfig` itself is immutable and shared across every tick, so
this is the actual mechanism by which a hook can influence the upcoming fetch (e.g. a
freshly refreshed bearer token), rather than running a side effect nothing reads.
Return an empty map if there's nothing to add. An error from the hook is logged as a
warning but does not skip the check.

### POST request support

```rust
cfg.request_body = Some(r#"{"query":"SELECT 1"}"#.into());
```

Setting `requestBody` switches the fetcher from GET to POST; strings are sent as-is,
and the `Content-Type: application/json` header is set automatically.

### Watcher tags

```rust
cfg.tags = vec!["prod".into(), "prices".into()];
```

```rust
wd.pause("prod");  // pauses every watcher tagged "prod"
wd.resume("prod");
```

### minInterval

```rust
cfg.min_interval = Some(30); // per-watcher floor
let wd = WatchDiff::new(WatchDiffOptions { min_interval: Some(30), ..Default::default() })?; // global floor
```

A watcher's effective `interval` is raised to the floor if it's set lower (per-watcher
`minInterval` takes precedence over the global one).

## WatchConfig reference

All fields live on `watchdiff_core::types::WatchConfig` (camelCase in JSON config files).
Defaults are shown where relevant.

| Field | Type | Default | Description |
|---|---|---|---|
| `url` | `String` | — | Target URL |
| `id` | `Option<String>` | — | Stable key for pause/resume/storage instead of URL |
| `label` | `Option<String>` | URL | Human-readable name |
| `target` | `Option<String>` | — | CSS selector, or an XPath expression (starting with `/` or `./`); omit for full page |
| `interval` | `u64` | `300` | Seconds between checks |
| `schedule` | `Option<String>` | — | 5-field cron expression, overrides `interval` |
| `headers` / `cookies` | maps | `{}` | Extra request headers / cookies |
| `timeout_ms` | `u64` | `15000` | Request timeout |
| `ignore_selectors` / `ignore_patterns` | `Vec<String>` | `[]` | Stripped before diffing |
| `proxies` / `user_agents` | `Vec<String>` | `[]` | Rotated per request |
| `diff_mode` | `DiffMode` | `Line` | `Line`/`Word`/`Semantic`/`Json`/`Rss` |
| `webhooks` | `Vec<String>` | `[]` | Webhook URLs |
| `min_changes` | `u32` | `1` | Minimum changes to trigger an alert |
| `cooldown` | `u64` | `0` | Minimum seconds between alerts |
| `retries` / `retry_delay_ms` / `max_retry_delay_ms` | — | `0` / `1000` / none | Fetch retry + backoff |
| `jitter` | `f64` | `0.0` | Interval randomisation, 0–1 |
| `dry_run` | `bool` | `false` | Fetch/diff without saving or alerting |
| `max_snapshots` | `Option<usize>` | — | Prune to last N snapshots |
| `change_threshold` | `f64` | `0.0` | Minimum fraction of content that must change |
| `ignore_numbers` | `bool` | `false` | Strip standalone numbers before diffing |
| `json_path` | `Option<String>` | — | Extract a sub-value, e.g. `"$.data.price"` |
| `alert_if_no_change_after` | `Option<u64>` | — | Fire `on_silence` after this many seconds without a detected change |
| `maintenance_windows` | `Vec<MaintenanceWindow>` | `[]` | Skip checks during these windows |
| `active_between` | `Option<ActiveBetween>` | — | Restrict checks to hours/days/timezone |
| `failure_policy` | `Option<FailurePolicy>` | — | Consecutive failures/successes before alert/recovery |
| `webhook_headers` | map | `{}` | Extra headers on outgoing webhook POSTs |
| `retention_days` | `Option<u32>` | — | Auto-prune snapshots older than N days |
| `auth` | `Option<BasicAuth>` | — | HTTP Basic Auth |
| `request_body` | `Option<String>` | — | Switches fetch to POST |
| `tags` | `Vec<String>` | `[]` | Grouping for pause/resume |
| `min_interval` | `Option<u64>` | — | Per-watcher floor on interval |
| `confirm_after` | `Option<u64>` | — | Flapping detection delay, seconds |
| `email` | `Option<EmailAlertConfig>` | — | SMTP alert config |
| `ai_summary` | `bool` | `false` | Enable AI summary (Anthropic only), attached to `DiffReport.ai_summary` |
| `ai_api_key` | `Option<String>` | env `ANTHROPIC_API_KEY` | Anthropic API key override |
| `ai_provider` | `AiProviderKind` | `Anthropic` | `Anthropic` \| `Openai` \| `Gemini` |
| `ai_prompt` | `Option<String>` | — | Override the default summary prompt |
| `allow_private_urls` | `bool` | `false` | Disable the SSRF guard |
| `max_response_size` | `Option<u64>` | `10 MB` | Response size cap |
| `archive_html` | `bool` | `false` | Save HTML to `<storageDir>/archive/` on change |
| `browser` | `bool` | `false` | Render via headless Chromium instead of a plain HTTP fetch |
| `browser_options` | `Option<BrowserOptions>` | — | `{ wait_for, wait_for_selector }` for browser-rendered checks |
| `screenshot_on_change` | `bool` | `false` | Save a PNG to `<storageDir>/archive/` on change; requires `browser: true` |
| `change_spike_window` / `change_spike_threshold` | — | — | Spike detection window/threshold |
| `expected_status` / `alert_on_status_change` / `track_response_time` | — | — | HTTP status monitoring |
| `alert_on_first_check` | `bool` | `false` | Fire `onChange` on the very first snapshot |
| `paused` | `bool` | `false` | Start paused (persisted via config file for the CLI) |

## Types

The core public types, all in `watchdiff_core::types` (re-exported from the crate root):

| Type | Purpose |
|---|---|
| `WatchConfig` | Per-watcher configuration — see [reference above](#watchconfig-reference) |
| `DiffReport` | `{ url, label, timestamp, changes, response_time_ms, http_status, ai_summary }` |
| `Change` | `{ kind, before, after, context }` — one diff entry |
| `Snapshot` | `{ url, content, timestamp, http_status }` — one stored content capture |
| `DiffMode` | `Line \| Word \| Semantic \| Json \| Rss` |
| `BrowserOptions` / `WaitFor` | `{ wait_for, wait_for_selector }` for `browser: true` watchers; `WaitFor` is `Load \| DomContentLoaded \| NetworkIdle` |
| `WatcherStatus` | Runtime status snapshot returned by `wd.status()` |
| `MaintenanceWindow` / `ActiveBetween` / `FailurePolicy` | Scheduling/alerting policy structs |
| `BasicAuth` / `SmtpConfig` / `EmailAlertConfig` | Auth/email configuration structs |
| `SpikeInfo` / `SilenceInfo` / `StatusChangeInfo` | Payloads passed to `on_spike`/`on_silence`/`on_status_change` |
| `db::DbChange`, `db::DbDiffMode`, `db::Row`, `db::ColumnDef`, `db::DbConn`, `db::Modification` | Database-monitoring types (`watchdiff_core::db`) — `DbConn` wraps SQLite/Postgres/MySQL; `Modification` is `DbChange::Updated`'s per-column `{ column, before, after }` |
| `cert::CertInfo` | `{ subject, serial, not_after, days_until_expiry }` — passed to `watch_cert`'s `on_change`/`on_warning` |
| `config_file::ConfigFile` | Programmatic access to the `watchdiff.config.json` format — `load`/`save`/`validate`/`example` (see [Config file](#config-file-watchdiffconfigjson)) |
| `AiProviderKind`, `ai::AiSummaryProvider` | `Anthropic \| Openai \| Gemini`; the trait each provider implements |

## Helper functions

```rust
use watchdiff_core::{report_summary, change_human};

println!("{}", report_summary(&report));   // "Book price (2 change(s))"
for c in &report.changes {
    println!("{}", change_human(c));       // "[~] Changed: \"19.99\" -> \"24.99\""
}
```

These back the message bodies sent to webhooks (`webhook.rs`) and are exposed publicly
for custom notifiers.

## CLI reference

| Command | Description |
|---|---|
| `watchdiff init [--config <path>]` | Write a starter `watchdiff.config.json` |
| `watchdiff run [<url>] [--config <path>] [--target] [--interval] [--dry-run] [--cooldown] [--archive-html] [--ignore-numbers] [--diff-mode] [--webhook <url>...] [--status-port <port>] [--browser] [--screenshot]` | Run watchers from a config file, or a single URL given directly |
| `watchdiff validate [--config <path>]` | Validate a config file (URLs, cron expressions, ranges) |
| `watchdiff status [--config <path>] [--json]` | Show last-change status per watcher |
| `watchdiff list [--config <path>]` | List configured watchers |
| `watchdiff diff <url> [--config <path>]` | Print the last stored diff report |
| `watchdiff snapshot <url> [--config <path>]` | Print the latest stored snapshot |
| `watchdiff compare <urlA> <urlB> [--target] [--diff-mode]` | Diff two arbitrary URLs directly, no storage |
| `watchdiff clean [--config <path>]` | Delete snapshot/report files not referenced by the config |
| `watchdiff pause <selector> [--config <path>]` | Pause a watcher (by id, url, or tag) in the config file |
| `watchdiff resume <selector> [--config <path>]` | Resume a watcher |
| `watchdiff export <url> [--config <path>] [--format json\|csv\|xlsx] [--snapshots] [--output <path>]` | Export reports (default) or snapshots; `--output` required for `xlsx` |

## Advanced usage

**Multiple watchers sharing one URL, addressed independently:**

```rust
let mut a = WatchConfig::new("https://example.com");
a.id = Some("example-price".into());
a.target = Some(".price".into());

let mut b = WatchConfig::new("https://example.com");
b.id = Some("example-stock".into());
b.target = Some(".stock".into());

wd.watch(a)?;
wd.watch(b)?;
wd.pause("example-price"); // only that watcher is paused
```

**Combining maintenance windows, active hours, and a failure policy:**

```rust
cfg.active_between = Some(ActiveBetween {
    from: "08:00".into(), to: "20:00".into(),
    days: vec![], // empty = every day
    timezone: Some("Europe/Paris".into()),
});
cfg.maintenance_windows = vec![MaintenanceWindow { from: deploy_start, to: deploy_end }];
cfg.failure_policy = Some(FailurePolicy { consecutive_failures: Some(3), consecutive_successes: Some(2) });
```

**Running the status server alongside the scheduler, and scraping it with Prometheus:**

```rust
wd.start_status_server(9090, None).await?;
let stop = wd.start();
```

```yaml
# prometheus.yml
scrape_configs:
  - job_name: watchdiff
    static_configs:
      - targets: ["127.0.0.1:9090"]
```

**Custom storage**: implement the `Store` trait (`store.rs`) for a backend other than
`JsonStore`/`SqliteStore` (e.g. Redis, Postgres-as-a-store) and pass it via
`WatchDiffOptions.store`.

## Optional dependencies

Unlike the TypeScript package — where Playwright, `better-sqlite3`, `pg`, `mysql2`,
`exceljs`, and `nodemailer` are optional `npm` peer dependencies installed on demand —
this is a compiled Rust binary: almost every feature it supports is statically linked
in by default, with one exception (XPath/libxml2, noted below).

| Area | Crate | Notes |
|---|---|---|
| HTTP fetch | `reqwest` | includes proxy/socks support |
| HTML parsing (CSS selectors) | `scraper` | pure Rust, statically linked, fully concurrent |
| XPath | `libxml` (libxml2 bindings) | needs `libxml2-dev`+`libclang` to **build**, and links `libxml2` **dynamically at runtime** — the one feature here with a real runtime dependency; see [Install](#install--build) and [XPath selectors](#xpath-selectors) |
| JS-heavy page rendering | `chromiumoxide` + `chromiumoxide_fetcher` | no extra build-time requirement; `browser: true` launches a Chrome/Chromium **subprocess at runtime** (auto-detected, or fetched on demand via `browser::fetch_chromium()`) — no browser process starts unless a watcher actually sets `browser: true` |
| SQLite | `rusqlite` (`bundled` feature) | compiles SQLite from source, no system lib needed |
| PostgreSQL / MySQL | `sqlx` (`postgres`/`mysql` features, `tls-rustls`) | native async drivers, no client library (`libpq`/`libmysqlclient`) needed |
| `NUMERIC`/`DECIMAL` columns | `rust_decimal` | used by the Postgres/MySQL column decoder |
| XLSX export | `rust_xlsxwriter` | pure Rust, no C dependency |
| AI summaries | none beyond `reqwest` | Anthropic/OpenAI/Gemini are all plain HTTPS JSON APIs |
| Email | `lettre` | STARTTLS via `rustls`, no external MTA |
| TLS certs | `tokio-rustls`, `x509-parser` | for SSL certificate monitoring |
| Status server | `axum` | always available |
| Cron | `cron` | always available |

Everything above (other than XPath and JS-heavy pages, both noted above with their own
runtime requirements) is statically linked in with zero extra runtime dependencies —
including PostgreSQL/MySQL support, which needs no client library installed since
`sqlx` implements both wire protocols natively in Rust.

## Project layout

```
src/
  types.rs              shared config/report/status types
  ssrf.rs                SSRF URL validation
  fetcher.rs             HTTP fetch, retries, proxy/UA rotation, caps
  cleaner.rs             HTML stripping, CSS/XPath extraction, ignores, jsonPath
  diff.rs                line/word/semantic/json/rss diff engines
  helpers.rs             report_summary / change_human
  store.rs               IStore trait + JSON-lines store
  sqlite_store.rs        SQLite-backed store
  webhook.rs             Discord/Slack/Telegram/Teams/ntfy senders
  email.rs               SMTP email alerts
  cert.rs                TLS certificate inspection
  sitemap.rs             sitemap.xml parsing + diffing
  db.rs                  SQLite/Postgres/MySQL table fetch/diff (row/schema/aggregate/value)
  ai.rs                  Anthropic / OpenAI / Gemini summary providers
  browser.rs             headless Chromium rendering (chromiumoxide) for browser: true
  status_server.rs       axum status/metrics HTTP server
  watchdiff.rs           WatchDiff orchestrator (web pipeline + scheduler)
  extra_watchers.rs      file/cert/sitemap/db scheduling (non-web pipelines)
  config_file.rs         watchdiff.config.json schema + validation
  bin/cli.rs             `watchdiff` CLI
```

## Testing

```bash
cargo test
```

Unit tests cover the diff engines (`diff.rs`), the HTML cleaner (`cleaner.rs` — CSS
selectors, XPath on both well-formed and deliberately malformed real-world HTML, and
the invalid-XPath error path), the recovery/failure-policy state machine
(`watchdiff.rs`), and the webhook message helpers (`helpers.rs`).

## Contributing

Missing a feature? Found a bug? Pull requests are welcome on GitHub.

If you want a feature that is not yet in the project, open an issue or submit a PR
directly — contributions of any size are appreciated.

## License

This project is licensed under the BSD 3-Clause License.

You are free to use, modify, and distribute this software in source and binary forms,
with or without modification, provided the copyright notice and disclaimer are
retained. No copyleft restrictions — compatible with MIT and proprietary projects.
