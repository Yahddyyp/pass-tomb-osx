use clap::Parser;

// Show, set, or clear timer status.
#[derive(Parser)]
#[command(
    name = "pass-timer",
    version = "1.0.0",
    about = "Show, set, or clear timer status"
)]
struct Cli {
    // Omit to show current timer.
    timer: Option<String>,

    // Path to the password store
    store: Option<String>,

    // Clear the timer
    #[arg(long = "clear")]
    clear: bool,

    // Be quiet
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    pass_tomb::store::cmd_timer(
        cli.timer.as_deref(),
        cli.store.as_deref(),
        cli.clear,
        cli.quiet,
    )
}
