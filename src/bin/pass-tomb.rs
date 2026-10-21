use clap::Parser;

#[derive(Parser)]
#[command(
    name = "pass-tomb",
    version = env!("CARGO_PKG_VERSION"),
    about = "A pass extension that helps keep the whole password tree encrypted inside a tomb."
)]
struct Cli {
    /// GPG key IDs used to encrypt the password store
    #[arg(value_name = "GPG-ID")]
    gpg_ids: Vec<String>,

    /// Change the configuration of an existing tomb
    #[arg(short = 'C', long = "change", conflicts_with_all = ["chkey", "resize", "export", "import", "install"])]
    change: bool,

    /// Change the GPG keys used to encrypt the tomb
    #[arg(long = "chkey", conflicts_with_all = ["change", "resize", "export", "import", "install"])]
    chkey: bool,

    /// Resize the encrypted tomb
    #[arg(long = "resize", value_name = "SIZE", conflicts_with_all = ["change", "chkey", "export", "import", "install"])]
    resize: Option<String>,

    /// Export the encrypted tomb or its supported backup representation
    #[arg(long = "export", conflicts_with_all = ["change", "chkey", "resize", "import", "install"])]
    export: bool,

    /// Specify the destination file for an export
    #[arg(long = "export-to", value_name = "FILE")]
    export_to: Option<String>,

    /// Import a tomb or backup from a file
    #[arg(long = "import", value_name = "FILE", conflicts_with_all = ["change", "chkey", "resize", "export", "install"])]
    import: Option<String>,

    /// Install the pass-tomb extension and its command wrappers
    #[arg(long = "install", conflicts_with_all = ["change", "chkey", "resize", "export", "import"])]
    install: bool,

    /// Do not initialize a new tomb if one does not exist
    #[arg(short = 'n', long = "no-init")]
    no_init: bool,

    /// Set the automatic tomb close timer
    #[arg(short = 'T', long = "timer")]
    timer: Option<String>,

    /// Specify the password store path
    #[arg(short = 'p', long = "path")]
    path: Option<String>,

    /// Specify the tomb file to use
    #[arg(short = 't', long = "tomb")]
    tomb: Option<String>,

    /// Specify the tomb key file
    #[arg(short = 'k', long = "key")]
    key: Option<String>,

    /// Specify the size of the tomb to create
    #[arg(short = 's', long = "size")]
    size: Option<String>,

    /// Force an operation without interactive confirmation where supported
    #[arg(short = 'f', long = "force")]
    force: bool,

    /// Be quiet
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Be verbose
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Enable debug output
    #[arg(short = 'd', long = "debug")]
    debug: bool,

    /// Allow operations that would otherwise be considered unsafe
    #[arg(long = "unsafe")]
    unsafe_mode: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.install {
        pass_tomb::store::cmd_install(cli.quiet)
    } else if cli.change {
        pass_tomb::store::cmd_change(
            cli.quiet,
            cli.verbose,
            cli.debug,
            cli.tomb.as_deref(),
            cli.key.as_deref(),
        )
    } else if cli.chkey {
        if cli.gpg_ids.is_empty() {
            anyhow::bail!("--chkey requires at least one GPG key ID");
        }
        pass_tomb::store::cmd_chkey(
            &cli.gpg_ids,
            cli.quiet,
            cli.verbose,
            cli.tomb.as_deref(),
            cli.key.as_deref(),
        )
    } else if let Some(size) = cli.resize {
        pass_tomb::store::cmd_resize(
            &size,
            cli.quiet,
            cli.verbose,
            cli.tomb.as_deref(),
            cli.key.as_deref(),
        )
    } else if cli.export {
        pass_tomb::store::cmd_export(
            cli.export_to.as_deref(),
            cli.quiet,
            cli.tomb.as_deref(),
            cli.key.as_deref(),
        )
    } else if let Some(src) = cli.import {
        pass_tomb::store::cmd_import(&src, cli.quiet, cli.tomb.as_deref(), cli.key.as_deref())
    } else if cli.gpg_ids.is_empty() {
        // No command and no GPG-ID — print help
        use clap::CommandFactory;
        Cli::command().print_help()?;
        Ok(())
    } else {
        pass_tomb::store::cmd_tomb(
            &cli.gpg_ids,
            cli.no_init,
            cli.timer.as_deref(),
            cli.path.as_deref(),
            cli.force,
            cli.quiet,
            cli.verbose,
            cli.debug,
            cli.unsafe_mode,
            cli.size.as_deref(),
            cli.tomb.as_deref(),
            cli.key.as_deref(),
        )
    }
}
