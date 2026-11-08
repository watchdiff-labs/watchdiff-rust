# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-08

Initial Rust port of [watchdiff-core](https://www.npmjs.com/package/watchdiff-core),
built over two days (2026-11-07 to 2026-11-08).

### Added

**Web monitoring pipeline**
- URL monitoring via `.watch(config)` + `.start()`
- CSS selectors (`scraper`) and XPath expressions (libxml2 via `libxml`, tolerant of
  real-world malformed HTML) for `target`, auto-detected from the string shape
- 5 diff modes: `line`, `word`, `semantic`, `json`, `rss` (RSS 2.0 / Atom, item-level
  by `guid`/`id`)
- Proxy rotation (`proxies`) and User-Agent rotation (`userAgents`), one picked at
  random per request
- Alert cooldown (`cooldown`) to rate-limit notifications per watcher
- Retry with exponential backoff (`retries`, `retryDelay`, `maxRetryDelay`)
- Interval jitter (`jitter`) to randomise polling and avoid thundering herds
- Dry-run mode (`dryRun`) — fetch and diff without saving or alerting
- Snapshot pruning (`maxSnapshots`) and automatic retention-based pruning
  (`retentionDays`)
- Change threshold (`changeThreshold`) — require a minimum fraction of content to
  change before alerting
- Ignore numbers (`ignoreNumbers`) and ignore patterns/selectors
  (`ignorePatterns`/`ignoreSelectors`)
- HTML archiving on change (`archiveHtml`) to `<storageDir>/archive/`
- HTTP status monitoring (`alertOnStatusChange`/`onStatusChange`), with
  `expectedStatus` and `trackResponseTime` → `report.responseTimeMs`
- JSON path targeting (`jsonPath`) to diff a single field inside a JSON response
- Cookie-based authentication (`cookies`, percent-encoded)
- HTTP Basic Auth (`auth: { user, pass }`)
- POST request support (`requestBody`) — switches the fetcher to POST
- Cron scheduling (`schedule`, standard 5-field expressions via the `cron` crate)
- Flapping detection (`confirmAfter`) — re-fetches and re-diffs before alerting,
  suppressing transient changes
- `alertOnFirstCheck` — fire `on_change` on the very first snapshot capture

**JS-heavy pages / browser rendering**
- Headless Chromium rendering (`chromiumoxide`, native CDP, async) via `browser: true`
- `browserOptions: { waitFor, waitForSelector }` (`Load`/`DomContentLoaded`/`NetworkIdle`)
- Screenshot on change (`screenshotOnChange`, requires `browser: true`), saved as PNG
  to `<storageDir>/archive/`
- Shared, lazily-launched headless Chromium instance per `WatchDiff` (not per check)
- Optional on-demand Chromium download (`browser::fetch_chromium()` +
  `WatchDiffOptions.browser_executable`) for environments with no system browser

**Other monitor types**
- File monitoring (`watch_file`) — same diff pipeline as web pages, local files
- API monitoring convenience (`watch_api`) — defaults `diffMode` to `json`, enables
  `trackResponseTime`
- SSL certificate monitoring (`watch_cert`) — alerts on certificate replacement
  (serial change) and upcoming expiry (`warnDaysBeforeExpiry`)
- Sitemap monitoring (`watch_sitemap`) — alerts on added/removed URLs in a
  `sitemap.xml`
- Database monitoring (`watch_db`) for **SQLite, PostgreSQL, and MySQL**:
  - `Row` diff mode — inserts/deletes/column-level updates by primary key
  - `Schema` diff mode — column name/type/nullability changes
  - `Aggregate` diff mode — alert when a numeric scalar query changes by ≥ N%
  - `Value` diff mode — alert when an arbitrary scalar query result changes
  - SELECT/PRAGMA-only query validation (`validate_select_only`) before execution
  - PostgreSQL/MySQL column decoding across common SQL types, including
    `NUMERIC`/`DECIMAL` via `rust_decimal`

**Alerts and notifications**
- Global and per-watcher `on_change` callbacks
- Error callback (`on_error`) in place of default console logging
- Condition-based alerts (`Handlers.alert_if`) — suppress an alert without consuming
  the cooldown
- Async pre-fetch hook (`Handlers.on_before_check`) that can inject extra headers
  (e.g. a freshly refreshed auth token) into that one check's request
- `onRecovery` callback, tied to a configurable failure policy
  (`consecutiveFailures`/`consecutiveSuccesses`)
- Silence detection (`alertIfNoChangeAfter` → `on_silence`)
- Change-spike detection (`changeSpikeWindow`/`changeSpikeThreshold` → `on_spike`)
- Webhook channels: Discord, Slack (Block Kit — header/section/context blocks),
  Telegram Bot API, Microsoft Teams (MessageCard), ntfy.sh, and a generic JSON POST
  fallback
- Custom webhook headers (`webhookHeaders`) and configurable webhook retry count
  (`webhookRetries`)
- Email alerts via SMTP (`lettre`, STARTTLS over `rustls`, no external MTA)
- AI summaries via **Anthropic Claude, OpenAI, or Gemini** (`aiSummary`,
  `ai_provider`), with:
  - Per-watcher API key override (`ai_api_key`) or environment variables
    (`ANTHROPIC_API_KEY`/`OPENAI_API_KEY`/`GEMINI_API_KEY`)
  - Static prompt override (`ai_prompt`) or a per-report dynamic prompt function
    (`Handlers.ai_prompt_fn`)
  - A shared `ai::AiSummaryProvider` trait so adding another provider doesn't touch
    the scheduler

**Scheduling and lifecycle**
- Pause/resume by stable id, URL, or tag (`wd.pause`/`wd.resume`)
- Watcher tags (`tags`) for group pause/resume
- Concurrency limiting (`WatchDiffOptions.concurrency`, a `tokio::sync::Semaphore`)
- Maintenance windows (`maintenanceWindows`) — skip checks entirely during a window
- Active hours (`activeBetween`) — restrict monitoring to specific hours/days, with
  full IANA timezone support via `chrono-tz`
- Per-watcher and global floor on check interval (`minInterval`)
- `maxWatchers` cap on the number of registered watchers
- Watcher status snapshot (`wd.status()` → `WatcherStatus`)

**Storage and export**
- JSON-lines storage backend (`JsonStore`, default) and SQLite backend
  (`SqliteStore`, `rusqlite` with the `bundled` feature)
- CSV export (`export_reports_csv`/`export_snapshots_csv`)
- JSON export (`export_reports_json`/`export_snapshots_json`)
- XLSX export (`export_reports_xlsx`/`export_snapshots_xlsx`, pure-Rust via
  `rust_xlsxwriter`, no C dependency)
- URL comparison (`compare_urls`) — one-shot diff between two arbitrary URLs with no
  storage side effects

**Security**
- SSRF guard (`ssrf::validate_url`) blocking loopback, link-local, private (RFC
  1918), CGNAT (100.64.0.0/10), and the IPv6 equivalents (`::1`, `fc00::/7`,
  `fe80::/10`) by default, with an explicit `allowPrivateUrls` opt-out
- Response size cap (`maxResponseSize`, default 10 MB)
- Redirect safety — capped at 10 hops, with the SSRF guard re-applied to **every**
  redirect target via a custom `reqwest::redirect::Policy`, not just the initial URL
- SQL injection guard for custom DB queries (SELECT/PRAGMA-only)
- Cookie value percent-encoding to prevent header injection

**Status server and CLI**
- Built-in HTTP status server (`startStatusServer`) exposing `/health`, `/status`,
  and `/metrics` (Prometheus text format), bound to `127.0.0.1` by default
- Full CLI: `init`, `run` (config file or single-URL one-liner, with `--browser`/
  `--screenshot`/`--status-port`/etc.), `validate`, `status` (`--json`), `list`,
  `diff`, `snapshot`, `compare`, `clean`, `pause`, `resume`, `export`
  (`--format json|csv|xlsx`, `--output`)
- `watchdiff.config.json` config file format, loadable/saveable/validatable
  programmatically via `config_file::ConfigFile`
- Config validation (`watchdiff validate`) — invalid URLs, unsupported/invalid
  CSS selectors or XPath expressions, bad cron expressions, out-of-range
  `changeThreshold`/`jitter`, `screenshotOnChange` without `browser: true`
- Helper functions `report_summary`/`change_human` for custom notifiers

### Security

- Every HTTP redirect hop is now re-validated against the SSRF guard via a custom
  `reqwest::redirect::Policy`, not just the initial URL — closing a bypass where a
  server could redirect an allowed public URL to an internal address
- Fixed the SSRF guard silently letting through **every literal IPv6 address**
  (not just loopback): `Url::host_str()` returns IPv6 literals bracketed
  (e.g. `"[::1]"`), which always failed `str::parse::<IpAddr>()`; switched to
  `Url::host()`, which returns an already-parsed address

### Known limitations

- The SSRF guard checks literal IP addresses in the URL; it does not perform DNS
  resolution, so a hostname that only *resolves* to a private address (DNS
  rebinding) is not caught — matching the scope of the equivalent guard in
  watchdiff-ts
- `WaitFor::NetworkIdle` is approximated as a fixed grace period after `load`,
  not true CDP network-idle detection; `waitForSelector` polls rather than using a
  MutationObserver-backed wait
- PostgreSQL/MySQL column decoding tries common SQL types in sequence; an
  unrecognized type falls back to an `<undecoded:TYPE>` placeholder string
- No multi-browser support for JS-heavy pages — Chromium only, no Firefox/WebKit
