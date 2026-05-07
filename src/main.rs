mod app;
mod cli;
mod command;
mod fs_utils;
mod installer;
mod patches;
mod ui;

use anyhow::Result;
use clap::Parser;
use console::style;

use crate::app::App;
use crate::cli::Cli;

fn main() {
    if let Err(error) = run() {
        eprintln!("{} {error:#}", style("Error:").red().bold());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let app = App::from_venv(cli.debug, cli.venv)?;

    installer::run_setup(
        &app,
        cli.purge,
        cli.skip_pre_commit_install,
        cli.skip_env_file,
    )
}
