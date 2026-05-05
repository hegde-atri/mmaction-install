use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use console::style;

use crate::app::App;

#[derive(Clone, Copy)]
pub(crate) enum OutputMode {
    Quiet,
    Stream,
}

pub(crate) fn run_command(
    app: &App,
    label: &str,
    mut command: Command,
    mode: OutputMode,
) -> Result<()> {
    let should_stream = app.debug || matches!(mode, OutputMode::Stream);

    if should_stream {
        let status = command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("failed to spawn command: {label}"))?;

        if status.success() {
            return Ok(());
        }

        bail!("command failed ({label}) with status {status}");
    }

    let output = command
        .output()
        .with_context(|| format!("failed to spawn command: {label}"))?;

    if output.status.success() {
        return Ok(());
    }

    eprintln!();
    eprintln!("{} {}", style("Command failed:").red().bold(), label);
    if !output.stdout.is_empty() {
        eprintln!("{}", style("--- stdout ---").yellow());
        eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprintln!("{}", style("--- stderr ---").yellow());
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    bail!("command failed ({label}) with status {}", output.status)
}
