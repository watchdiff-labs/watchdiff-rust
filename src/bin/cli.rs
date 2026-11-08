use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use watchdiff_core::config_file::ConfigFile;
use watchdiff_core::store::{JsonStore, Store};
use watchdiff_core::types::{DiffMode, WatchConfig};
use watchdiff_core::{WatchDiff, WatchDiffOptions};

#[derive(Parser)]
#[command(name = "watchdiff", about = "Lightweight web/file/API/DB/SSL change monitoring")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a starter watchdiff.config.json
    Init {
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Run watchers from a config file, or a single URL given directly
    Run {
        url: Option<String>,
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value_t = 300)]
        interval: u64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 0)]
        cooldown: u64,
        #[arg(long)]
        archive_html: bool,
        #[arg(long)]
        ignore_numbers: bool,
        #[arg(long, value_enum)]
        diff_mode: Option<DiffModeArg>,
        #[arg(long)]
        webhook: Vec<String>,
        #[arg(long)]
        status_port: Option<u16>,
        /// Render via headless Chromium instead of a plain HTTP fetch
        #[arg(long)]
        browser: bool,
        /// Save a PNG screenshot on change (requires --browser)
        #[arg(long)]
        screenshot: bool,
    },
    /// Validate a config file for errors
    Validate {
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Show watcher status
    Status {
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List configured watchers
    List {
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Print the last diff report for a URL
    Diff {
        url: String,
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Print the latest stored snapshot for a URL
    Snapshot {
        url: String,
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Diff two arbitrary URLs directly
    Compare {
        url_a: String,
        url_b: String,
        #[arg(long)]
        target: Option<String>,
        #[arg(long, value_enum, default_value = "line")]
        diff_mode: DiffModeArg,
    },
    /// Delete orphan snapshot files not referenced by the config
    Clean {
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Pause a watcher (by id, url, or tag)
    Pause {
        selector: String,
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Resume a watcher (by id, url, or tag)
    Resume {
        selector: String,
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
    },
    /// Export stored reports or snapshots
    Export {
        url: String,
        #[arg(long, default_value = "watchdiff.config.json")]
        config: PathBuf,
        #[arg(long, default_value = "json")]
        format: String,
        #[arg(long)]
        snapshots: bool,
        /// Required for --format xlsx (binary); optional for json/csv (prints to stdout if omitted)
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum DiffModeArg {
    Line,
    Word,
    Semantic,
    Json,
    Rss,
}

impl From<DiffModeArg> for DiffMode {
    fn from(v: DiffModeArg) -> Self {
        match v {
            DiffModeArg::Line => DiffMode::Line,
            DiffModeArg::Word => DiffMode::Word,
            DiffModeArg::Semantic => DiffMode::Semantic,
            DiffModeArg::Json => DiffMode::Json,
            DiffModeArg::Rss => DiffMode::Rss,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { config } => {
            if config.exists() {
                println!("{} already exists", config.display());
                return Ok(());
            }
            ConfigFile::example().save(&config)?;
            println!("Created {}", config.display());
        }

        Commands::Run {
            url,
            config,
            target,
            interval,
            dry_run,
            cooldown,
            archive_html,
            ignore_numbers,
            diff_mode,
            webhook,
            status_port,
            browser,
            screenshot,
        } => {
            let cfg_file = if let Some(url) = url {
                let mut watcher           = WatchConfig::new(url);
                watcher.target            = target;
                watcher.interval          = interval;
                watcher.dry_run           = dry_run;
                watcher.cooldown          = cooldown;
                watcher.archive_html      = archive_html;
                watcher.ignore_numbers    = ignore_numbers;
                watcher.diff_mode         = diff_mode.map(DiffMode::from).unwrap_or_default();
                watcher.webhooks          = webhook;
                watcher.browser           = browser;
                watcher.screenshot_on_change = screenshot;
                ConfigFile {
                    storage_dir: ".watchdiff".to_string(),
                    concurrency: None,
                    min_interval: None,
                    watchers: vec![watcher],
                }
            } else {
                ConfigFile::load(&config)?
            };

            let errors = cfg_file.validate();
            if !errors.is_empty() {
                for e in &errors {
                    eprintln!("config error: {e}");
                }
                anyhow::bail!("refusing to run with invalid config");
            }

            let wd = WatchDiff::new(WatchDiffOptions {
                storage_dir: cfg_file.storage_dir.clone(),
                store: None,
                concurrency: cfg_file.concurrency,
                min_interval: cfg_file.min_interval,
                max_watchers: None,
                browser_executable: None,
            })?;

            for watcher in cfg_file.watchers {
                let label = watcher.effective_label();
                println!("Watching {} ({label})", watcher.url);
                wd.watch(watcher)?;
            }

            if let Some(port) = status_port {
                wd.start_status_server(port, None).await?;
                println!("Status server listening on http://127.0.0.1:{port}");
            }

            let stop = wd.start();
            println!("Running. Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await?;
            stop();
        }

        Commands::Validate { config } => {
            let cfg_file    = ConfigFile::load(&config)?;
            let errors      = cfg_file.validate();
            if errors.is_empty() {
                println!("OK: {} watcher(s), no errors found", cfg_file.watchers.len());
            } else {
                for e in &errors {
                    println!("ERROR: {e}");
                }
                std::process::exit(1);
            }
        }

        Commands::Status { config, json } => {
            let cfg_file        = ConfigFile::load(&config)?;
            let store           = Arc::new(JsonStore::new(&cfg_file.storage_dir)?);
            let rows: Vec<_>    = cfg_file
                .watchers
                .iter()
                .map(|w| {
                    let key            = w.key();
                    let reports        = store.load_reports(&key, Some(1)).unwrap_or_default();
                    let last_change    = reports.last().map(|r| r.timestamp.to_rfc3339());
                    serde_json::json!({
                        "url": w.url,
                        "label": w.effective_label(),
                        "paused": w.paused,
                        "interval": w.interval,
                        "lastChangeAt": last_change,
                    })
                })
                .collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for r in &rows {
                    println!(
                        "{} [{}] interval={}s paused={} lastChange={}",
                        r["label"], r["url"], r["interval"], r["paused"],
                        r["lastChangeAt"].as_str().unwrap_or("never")
                    );
                }
            }
        }

        Commands::List { config } => {
            let cfg_file = ConfigFile::load(&config)?;
            for w in &cfg_file.watchers {
                println!(
                    "{}\turl={}\ttarget={}\tinterval={}s\ttags={}\tpaused={}",
                    w.effective_label(),
                    w.url,
                    w.target.clone().unwrap_or_else(|| "-".into()),
                    w.interval,
                    if w.tags.is_empty() { "-".into() } else { w.tags.join(",") },
                    w.paused
                );
            }
        }

        Commands::Diff { url, config } => {
            let cfg_file    = ConfigFile::load(&config)?;
            let store       = JsonStore::new(&cfg_file.storage_dir)?;
            let reports     = store.load_reports(&url, Some(1))?;
            match reports.last() {
                Some(r) => println!("{}", serde_json::to_string_pretty(r)?),
                None => println!("No reports found for {url}"),
            }
        }

        Commands::Snapshot { url, config } => {
            let cfg_file    = ConfigFile::load(&config)?;
            let store       = JsonStore::new(&cfg_file.storage_dir)?;
            let snaps       = store.load_snapshots(&url, Some(1))?;
            match snaps.last() {
                Some(s) => println!("{}", serde_json::to_string_pretty(s)?),
                None => println!("No snapshot found for {url}"),
            }
        }

        Commands::Compare { url_a, url_b, target, diff_mode } => {
            let wd        = WatchDiff::new(WatchDiffOptions::default())?;
            let report    = wd.compare_urls(&url_a, &url_b, diff_mode.into(), target.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }

        Commands::Clean { config } => {
            let cfg_file                                           = ConfigFile::load(&config)?;
            let snapshots_dir                                      = PathBuf::from(&cfg_file.storage_dir).join("snapshots");
            let reports_dir                                        = PathBuf::from(&cfg_file.storage_dir).join("reports");
            let known_hashes: std::collections::HashSet<String>    = cfg_file
                .watchers
                .iter()
                .map(|w| watchdiff_core::store::hash_key(&w.key()))
                .collect();

            let mut removed = 0;
            for dir in [&snapshots_dir, &reports_dir] {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let stem = entry.path().file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        if !known_hashes.contains(&stem) {
                            std::fs::remove_file(entry.path()).ok();
                            removed += 1;
                        }
                    }
                }
            }
            println!("Removed {removed} orphan file(s)");
        }

        Commands::Pause { selector, config } => {
            toggle_paused(&config, &selector, true)?;
            println!("Paused: {selector}");
        }

        Commands::Resume { selector, config } => {
            toggle_paused(&config, &selector, false)?;
            println!("Resumed: {selector}");
        }

        Commands::Export { url, config, format, snapshots, output: output_path } => {
            let cfg_file    = ConfigFile::load(&config)?;
            let store       = JsonStore::new(&cfg_file.storage_dir)?;

            if format == "xlsx" {
                let Some(out_path) = output_path else {
                    anyhow::bail!("--format xlsx requires --output <path> (binary format can't print to stdout)");
                };
                use rust_xlsxwriter::Workbook;
                let mut workbook = Workbook::new();
                let worksheet = workbook.add_worksheet();
                if snapshots {
                    let rows = store.load_snapshots(&url, None)?;
                    for (col, header) in ["timestamp", "url", "http_status", "content"].iter().enumerate() {
                        worksheet.write(0, col as u16, *header)?;
                    }
                    for (i, s) in rows.iter().enumerate() {
                        let row = (i + 1) as u32;
                        worksheet.write(row, 0, s.timestamp.to_rfc3339())?;
                        worksheet.write(row, 1, s.url.as_str())?;
                        worksheet.write(row, 2, s.http_status.map(|v| v.to_string()).unwrap_or_default())?;
                        worksheet.write(row, 3, s.content.as_str())?;
                    }
                } else {
                    let rows = store.load_reports(&url, None)?;
                    for (col, header) in ["timestamp", "url", "label", "kind", "before", "after", "context"].iter().enumerate() {
                        worksheet.write(0, col as u16, *header)?;
                    }
                    let mut row = 1u32;
                    for r in &rows {
                        for c in &r.changes {
                            worksheet.write(row, 0, r.timestamp.to_rfc3339())?;
                            worksheet.write(row, 1, r.url.as_str())?;
                            worksheet.write(row, 2, r.label.as_str())?;
                            worksheet.write(row, 3, c.kind.as_str())?;
                            worksheet.write(row, 4, c.before.as_deref().unwrap_or(""))?;
                            worksheet.write(row, 5, c.after.as_deref().unwrap_or(""))?;
                            worksheet.write(row, 6, c.context.as_deref().unwrap_or(""))?;
                            row += 1;
                        }
                    }
                }
                workbook.save(&out_path)?;
                println!("Wrote {}", out_path.display());
                return Ok(());
            }

            let output = match (snapshots, format.as_str()) {
                (true, "csv") => {
                    let rows       = store.load_snapshots(&url, None)?;
                    let mut wtr    = csv::Writer::from_writer(vec![]);
                    wtr.write_record(["timestamp", "url", "http_status", "content"])?;
                    for s in rows {
                        wtr.write_record([
                            s.timestamp.to_rfc3339(),
                            s.url,
                            s.http_status.map(|v| v.to_string()).unwrap_or_default(),
                            s.content,
                        ])?;
                    }
                    String::from_utf8(wtr.into_inner()?)?
                }
                (true, _) => serde_json::to_string_pretty(&store.load_snapshots(&url, None)?)?,
                (false, "csv") => {
                    let rows       = store.load_reports(&url, None)?;
                    let mut wtr    = csv::Writer::from_writer(vec![]);
                    wtr.write_record(["timestamp", "url", "label", "kind", "before", "after", "context"])?;
                    for r in rows {
                        for c in &r.changes {
                            wtr.write_record([
                                r.timestamp.to_rfc3339(),
                                r.url.clone(),
                                r.label.clone(),
                                c.kind.clone(),
                                c.before.clone().unwrap_or_default(),
                                c.after.clone().unwrap_or_default(),
                                c.context.clone().unwrap_or_default(),
                            ])?;
                        }
                    }
                    String::from_utf8(wtr.into_inner()?)?
                }
                (false, _) => serde_json::to_string_pretty(&store.load_reports(&url, None)?)?,
            };
            match &output_path {
                Some(path) => {
                    std::fs::write(path, &output)?;
                    println!("Wrote {}", path.display());
                }
                None => println!("{output}"),
            }
        }
    }

    Ok(())
}

fn toggle_paused(config_path: &PathBuf, selector: &str, paused: bool) -> anyhow::Result<()> {
    let mut cfg_file    = ConfigFile::load(config_path)?;
    let mut matched     = false;
    for w in &mut cfg_file.watchers {
        if w.key() == selector || w.url == selector || w.tags.iter().any(|t| t == selector) {
            w.paused    = paused;
            matched     = true;
        }
    }
    if !matched {
        anyhow::bail!("no watcher matched '{selector}'");
    }
    cfg_file.save(config_path)?;
    Ok(())
}
