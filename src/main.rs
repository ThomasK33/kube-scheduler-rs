#![forbid(unsafe_code)]

mod scheduler;
mod utils;

use clap::{Parser, ValueEnum};
use clap_verbosity_flag::InfoLevel;
use color_eyre::Result;

use scheduler::run_scheduler;
use utils::convert_filter;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity<InfoLevel>,

    #[arg(value_enum, long, env, default_value_t = Algorithm::BinPacking)]
    algorithm: Algorithm,

    #[arg(long, env, default_value = "kube-scheduler-rs")]
    scheduler_name: String,

    /// Debounce duration in seconds
    #[arg(long, env, default_value_t = 3)]
    debounce_duration: u64,

    /// Debounce timeout in seconds after which a scheduler run is triggered anyways
    #[arg(long, env, default_value_t = 10)]
    debounce_timeout: u64,
}

#[derive(ValueEnum, Clone, Debug)]
enum Algorithm {
    BinPacking,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(convert_filter(cli.verbose.log_level_filter()))
        .init();

    tracing::info!("Launching scheduler: {}", env!("CARGO_PKG_VERSION"));
    run_scheduler(cli).await
}
