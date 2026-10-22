use clap::Parser;

// Close a password tomb.
#[derive(Parser)]
#[command(
    name = "pass-close",
    version = env!("CARGO_PKG_VERSION"),
    about = "Close a password tomb."
)]
struct Cli {
    /// Path to the password store or tomb file to close
    store: Option<String>,

    /// Be quiet
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Be verbose
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    pass_tomb::store::cmd_close(cli.store.as_deref(), cli.quiet, cli.verbose)
}
