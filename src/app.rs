use std::path::PathBuf;

use anyhow::{Context, Result, bail};

pub(crate) struct App {
    pub(crate) debug: bool,
    pub(crate) venv_dir: PathBuf,
    pub(crate) venv_was_provided: bool,
}

impl App {
    pub(crate) fn from_venv(debug: bool, venv: Option<PathBuf>) -> Result<Self> {
        let (venv_dir, venv_was_provided) = resolve_venv_path(venv)?;

        Ok(Self {
            debug,
            venv_dir,
            venv_was_provided,
        })
    }

    pub(crate) fn python_bin(&self) -> PathBuf {
        self.venv_dir.join("bin/python")
    }
}

fn resolve_venv_path(venv: Option<PathBuf>) -> Result<(PathBuf, bool)> {
    let venv_was_provided = venv.is_some();
    let raw_path = venv.unwrap_or_else(|| PathBuf::from(".venv"));
    let venv_dir = if raw_path.is_absolute() {
        raw_path
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory for --venv")?
            .join(raw_path)
    };

    if venv_dir.exists() && !venv_dir.is_dir() {
        bail!(
            "virtual environment path is not a directory: {}",
            venv_dir.display()
        );
    }

    Ok((venv_dir, venv_was_provided))
}
