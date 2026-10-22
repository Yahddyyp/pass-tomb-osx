use clap::Parser;

#[derive(Parser)]
#[command(
    name = "pass-timer",
    version = env!("CARGO_PKG_VERSION"),
    about = "Show, set, or clear a tomb-specific persistent timer."
)]
struct Cli {
    /// Omit to show current timer
    timer: Option<String>,

    /// Path to the password store
    store: Option<String>,

    /// Tomb image whose persistent timer should be read or changed
    #[arg(short = 't', long = "tomb", value_name = "FILE")]
    tomb: Option<String>,

    /// Clear the timer
    #[arg(long = "clear")]
    clear: bool,

    /// Be quiet
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    pass_tomb::store::cmd_timer(
        cli.timer.as_deref(),
        cli.store.as_deref(),
        cli.tomb.as_deref(),
        cli.clear,
        cli.quiet,
    )
}
