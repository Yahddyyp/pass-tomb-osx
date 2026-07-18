use clap::Parser;

// Close a password tomb.
#[derive(Parser)]
#[command(
    name = "pass-close",
    version = "1.0.0",
    about = "Close a password tomb"
)]
struct Cli {
    // Path to the password store or tomb file to close
    store: Option<String>,

    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    pass_tomb::store::cmd_close(cli.store.as_deref(), cli.quiet, cli.verbose)
}
