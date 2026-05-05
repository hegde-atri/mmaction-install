use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::app::App;

pub(crate) fn print_header(app: &App) {
    println!(
        "{} {}",
        style("./setup").cyan().bold(),
        style("CLI that installs mmaction stack with local wheel builds and runs uv sync").dim()
    );
    println!(
        "{} {}",
        style("•").cyan(),
        if app.debug {
            style("Debug output: enabled").yellow().to_string()
        } else {
            style("Debug output: disabled").dim().to_string()
        }
    );
    println!(
        "{} {} {}",
        style("•").cyan(),
        style("Virtual env:").dim(),
        style(app.venv_dir.display()).dim()
    );
}

pub(crate) fn run_step<F>(index: usize, total: usize, name: &str, debug: bool, f: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let started_at = Instant::now();

    if debug {
        println!(
            "{} [{index}/{total}] {}",
            style("→").cyan().bold(),
            style(name).cyan()
        );
        return match f() {
            Ok(()) => {
                let elapsed = format_elapsed(started_at.elapsed());
                println!(
                    "{} [{index}/{total}] {} {}",
                    style("✔").green().bold(),
                    style(name).green(),
                    style(format!("({elapsed})")).dim()
                );
                Ok(())
            }
            Err(error) => {
                let elapsed = format_elapsed(started_at.elapsed());
                println!(
                    "{} [{index}/{total}] {} {}",
                    style("✖").red().bold(),
                    style(name).red(),
                    style(format!("({elapsed})")).dim()
                );
                Err(error).map_err(|error| anyhow!(error).context(format!("step failed: {name}")))
            }
        };
    }

    let tick_set = &[
        "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
    ];

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan.bold} {prefix:.dim} {msg} {elapsed_precise:.dim}",
        )
        .expect("valid spinner template")
        .tick_strings(tick_set),
    );
    spinner.enable_steady_tick(Duration::from_millis(90));
    spinner.set_prefix(format!("[{index}/{total}]"));
    spinner.set_message(name.to_string());

    match f() {
        Ok(()) => {
            let elapsed = format_elapsed(started_at.elapsed());
            spinner.finish_with_message(format!(
                "{} [{index}/{total}] {} {}",
                style("✔").green().bold(),
                style(name).green(),
                style(format!("({elapsed})")).dim()
            ));
            Ok(())
        }
        Err(error) => {
            let elapsed = format_elapsed(started_at.elapsed());
            spinner.finish_with_message(format!(
                "{} [{index}/{total}] {} {}",
                style("✖").red().bold(),
                style(name).red(),
                style(format!("({elapsed})")).dim()
            ));
            Err(error).map_err(|error| anyhow!(error).context(format!("step failed: {name}")))
        }
    }
}

fn format_elapsed(duration: Duration) -> String {
    if duration.as_secs() < 60 {
        format!("{:.1}s", duration.as_secs_f32())
    } else {
        let mins = duration.as_secs() / 60;
        let secs = duration.as_secs() % 60;
        format!("{mins}m {secs}s")
    }
}
