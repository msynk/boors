//! `boors` — Boors market-data developer CLI (same commands and options as the .NET `Boors.Market.Cli`).

mod collect;
mod fetch;
mod market;
mod output;
mod verify;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use boors_market::sources::CachePolicy;
use boors_market::{Error, MarketContext, MarketDataOptions};
use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "boors", version, about = "boors — Boors market-data developer CLI", disable_help_subcommand = true)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Debug, Clone)]
pub struct Global {
    /// Data root for the raw cache and outputs (default: .boors-data, or BOORS_DATA).
    #[arg(long, global = true, env = "BOORS_DATA")]
    pub data: Option<PathBuf>,

    /// Collector node name recorded on every raw entry (default: machine name, or BOORS_NODE).
    #[arg(long, global = true, env = "BOORS_NODE")]
    pub node: Option<String>,

    /// Use cached responses when present (resume a long fetch).
    #[arg(long, global = true)]
    pub cache: bool,

    /// Use cached responses only; never touch the network.
    #[arg(long, global = true)]
    pub offline: bool,

    /// Log every request.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Optional rules.yaml overriding the embedded copy.
    #[arg(long, global = true)]
    pub rules: Option<PathBuf>,
}

impl Global {
    pub fn policy(&self) -> CachePolicy {
        if self.offline {
            CachePolicy::CacheOnly
        } else if self.cache {
            CachePolicy::PreferCache
        } else {
            CachePolicy::Network
        }
    }

    pub fn data_root(&self) -> PathBuf {
        self.data.clone().unwrap_or_else(|| PathBuf::from(".boors-data"))
    }

    pub fn options(&self) -> MarketDataOptions {
        let mut options = MarketDataOptions { data_root: self.data_root(), rules_path: self.rules.clone(), ..MarketDataOptions::default() };
        if let Some(node) = self.node.clone().filter(|n| !n.trim().is_empty()) {
            options.node = node;
        }
        options
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Check reachability of every data source from this node.
    Probe,
    /// Phase 0 inspection commands.
    #[command(subcommand)]
    Fetch(fetch::FetchCommand),
    /// Data-spec, adjustment, schema-drift and store-vs-TSETMC checks.
    #[command(subcommand)]
    Verify(verify::VerifyCommand),
    /// Run a collection job into the market store.
    Collect(collect::CollectArgs),
    /// Read-side analytics over the store (what the workstation pages show).
    #[command(subcommand)]
    Market(market::MarketCommand),
    /// Delete the store and rebuild it from the raw cache only (no network).
    Rebuild {
        /// Backfill at most N instruments.
        #[arg(long)]
        max: Option<usize>,
    },
    /// Dataset freshness, last job runs and last probe for this node.
    Health,
    /// Run SQL against the market store.
    Query {
        sql: String,
        #[arg(long, default_value_t = 200)]
        max: usize,
    },
    /// Move raw-cache entries between nodes.
    #[command(subcommand)]
    Cache(collect::CacheCommand),
    /// Import data without a machine-readable source.
    #[command(subcommand)]
    Import(collect::ImportCommand),
}

#[cfg(windows)]
fn utf8_console() {
    // Persian text: make the console code page UTF-8 for this process and children (std already writes UTF-16 to
    // real consoles; this matters when output is piped through other tools).
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleOutputCP(windows_sys::Win32::Globalization::CP_UTF8);
        windows_sys::Win32::System::Console::SetConsoleCP(windows_sys::Win32::Globalization::CP_UTF8);
    }
}

#[cfg(not(windows))]
fn utf8_console() {}

fn main() -> ExitCode {
    utf8_console();
    let cli = Cli::parse();
    let filter = if cli.global.verbose { "boors_market=debug,boors=debug,info" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("BOORS_LOG").unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)))
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .init();

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let code = runtime.block_on(async {
        tokio::select! {
            code = run(cli) => code,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("Cancelled.");
                Ok(130)
            }
        }
    });

    match code {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Opens the context (store, cache, clients) for the commands that need it.
pub fn open_context(global: &Global) -> anyhow::Result<Arc<MarketContext>> {
    Ok(MarketContext::open(global.options())?)
}

async fn run(cli: Cli) -> anyhow::Result<u8> {
    let global = cli.global.clone();
    let result = match cli.command {
        Command::Probe => collect::probe(&global).await,
        Command::Fetch(command) => fetch::run(&global, command).await,
        Command::Verify(command) => verify::run(&global, command).await,
        Command::Collect(args) => collect::collect(&open_context(&global)?, &args, global.policy()).await,
        Command::Market(command) => market::run(&open_context(&global)?, command),
        Command::Rebuild { max } => collect::rebuild(&global, max).await,
        Command::Health => collect::health(&open_context(&global)?),
        Command::Query { sql, max } => collect::query(&open_context(&global)?, &sql, max),
        Command::Cache(command) => collect::cache(&global, command),
        Command::Import(command) => collect::import(&open_context(&global)?, command).await,
    };

    match result {
        Ok(code) => Ok(code),
        Err(e) => match e.downcast_ref::<Error>() {
            Some(
                Error::SourceRequest(_)
                | Error::NotCached(_)
                | Error::Http(_)
                | Error::Rejected(_)
                | Error::Argument(_)
                | Error::Db(_)
                | Error::NotFound(_)
                | Error::InvalidOperation(_),
            ) => {
                eprintln!("error: {e}");
                Ok(2)
            }
            _ => Err(e),
        },
    }
}
