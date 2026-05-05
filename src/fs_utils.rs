use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use glob::glob;

pub(crate) fn wheel_exists(wheelhouse: &str, name: &str, version: &str) -> Result<bool> {
    let pattern = format!("{wheelhouse}/{name}-{version}-*.whl");
    let mut entries = glob(&pattern).with_context(|| format!("invalid glob pattern: {pattern}"))?;
    Ok(entries.next().transpose()?.is_some())
}

pub(crate) fn remove_dir_if_exists(path: &str) -> Result<()> {
    let dir = PathBuf::from(path);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove directory: {}", dir.display()))?;
    }
    Ok(())
}

pub(crate) fn purge_cache_dirs(wheelhouse: &str) -> Result<()> {
    for path in [wheelhouse, ".mmaction2", ".mmengine", ".mmcv"] {
        remove_dir_if_exists(path)?;
    }
    Ok(())
}
