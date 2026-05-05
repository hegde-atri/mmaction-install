use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "setup",
    author,
    version,
    about = "Install mmaction stack with local wheel builds and run uv sync"
)]
pub(crate) struct Cli {
    #[arg(
        long,
        default_value_t = false,
        help = "Show command output while running setup"
    )]
    pub(crate) debug: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Delete .wheelhouse, .mmaction2, .mmengine, and .mmcv before reinstalling"
    )]
    pub(crate) purge: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Virtual environment path for uv (relative or absolute)"
    )]
    pub(crate) venv: Option<PathBuf>,
}
