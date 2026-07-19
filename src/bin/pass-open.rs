use clap::Parser;

// Open a password tomb.
#[derive(Parser)]
#[command(name = "pass-open", version = "1.0.0", about = "Open a password tomb")]
struct Cli {
    // Subfolder to open the tomb in
    subfolder: Option<String>,

    // Specify the path to the password tomb
    #[arg(short = 't', long = "tomb")]
    tomb: Option<String>,

    // Specify the path to the password tomb key
    #[arg(short = 'k', long = "key")]
    key: Option<String>,

    // Close the store after a given time
    #[arg(short = 'T', long = "timer")]
    timer: Option<String>,

    // Force operation
    #[arg(short = 'f', long = "force")]
    force: bool,

    // Be quiet
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    // Be verbose
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    pass_tomb::store::cmd_open(
        cli.subfolder.as_deref(),
        cli.tomb.as_deref(),
        cli.key.as_deref(),
        cli.timer.as_deref(),
        cli.force,
        cli.quiet,
        cli.verbose,
    )
}
