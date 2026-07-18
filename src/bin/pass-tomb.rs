use clap::Parser;

#[derive(Parser)]
#[command(
    name = "pass-tomb",
    version = "1.0.0",
    about = "A pass extension that helps keep the whole password tree encrypted inside a tomb.

Made by Yahddyyp",
    override_usage = "
    pass tomb [OPTIONS] [GPG-ID]...
    pass open [subfolder] [-t tomb] [-k key] [-T time] [-f]
    pass close [store]
    pass timer [store]"
)]
struct Cli {
    #[arg(value_name = "GPG-ID")]
    gpg_ids: Vec<String>,

    #[arg(short = 'C', long = "change", conflicts_with_all = ["chkey", "resize", "export", "import", "install"])]
    change: bool,

    #[arg(long = "chkey", conflicts_with_all = ["change", "resize", "export", "import", "install"])]
    chkey: bool,

    #[arg(long = "resize", value_name = "SIZE", conflicts_with_all = ["change", "chkey", "export", "import", "install"])]
    resize: Option<String>,

    #[arg(long = "export", conflicts_with_all = ["change", "chkey", "resize", "import", "install"])]
    export: bool,

    #[arg(long = "export-to", value_name = "FILE")]
    export_to: Option<String>,

    #[arg(long = "import", value_name = "FILE", conflicts_with_all = ["change", "chkey", "resize", "export", "install"])]
    import: Option<String>,

    #[arg(long = "install", conflicts_with_all = ["change", "chkey", "resize", "export", "import"])]
    install: bool,

    #[arg(short = 'n', long = "no-init")]
    no_init: bool,

    #[arg(short = 'T', long = "timer")]
    timer: Option<String>,

    #[arg(short = 'p', long = "path")]
    path: Option<String>,

    #[arg(short = 't', long = "tomb")]
    tomb: Option<String>,

    #[arg(short = 'k', long = "key")]
    key: Option<String>,

    #[arg(short = 's', long = "size")]
    size: Option<String>,

    #[arg(short = 'f', long = "force")]
    force: bool,

    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    #[arg(short = 'd', long = "debug")]
    debug: bool,

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
        println!();
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
