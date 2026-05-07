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
        default_value_t = false,
        help = "Skip installing the local pre-commit hook"
    )]
    pub(crate) skip_pre_commit_install: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Skip creating the project .env file"
    )]
    pub(crate) skip_env_file: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Virtual environment path for uv (relative or absolute)"
    )]
    pub(crate) venv: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skip_pre_commit_install_flag() {
        let cli = Cli::try_parse_from(["setup", "--skip-pre-commit-install"]).unwrap();

        assert!(cli.skip_pre_commit_install);
    }

    #[test]
    fn parses_skip_env_file_flag() {
        let cli = Cli::try_parse_from(["setup", "--skip-env-file"]).unwrap();

        assert!(cli.skip_env_file);
    }
}
