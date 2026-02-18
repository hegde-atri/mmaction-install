use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use console::style;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};

const MMC_VERSION: &str = "2.1.0";
const MMACTION_VERSION: &str = "1.2.0";
const MMENGINE_VERSION: &str = "0.10.7";
const WHEELHOUSE: &str = ".wheelhouse";
const PYTHON_BIN: &str = ".venv/bin/python";

#[derive(Parser, Debug)]
#[command(author, version, about = "Install mmaction stack with local wheel builds")]
struct Cli {
    #[arg(long, default_value_t = false)]
    debug: bool,
}

#[derive(Clone, Copy)]
enum OutputMode {
    Quiet,
    Stream,
}

struct App {
    debug: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{} {error:#}", style("Error:").red().bold());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let app = App { debug: cli.debug };

    print_header(cli.debug);

    run_step(1, 8, "Ensuring wheelhouse directory", cli.debug, || {
        fs::create_dir_all(WHEELHOUSE).context("failed to create .wheelhouse directory")
    })?;

    run_step(2, 8, "Checking uv availability", cli.debug, check_uv)?;
    run_step(3, 8, "Ensuring Python virtual environment", cli.debug, || {
        ensure_venv(&app)
    })?;
    run_step(4, 8, "Ensuring pip tooling", cli.debug, || ensure_pip_tooling(&app))?;
    run_step(5, 8, "Building/installing mmcv", cli.debug, || {
        build_and_install_mmcv(&app)
    })?;
    run_step(6, 8, "Building/installing mmaction2", cli.debug, || {
        build_and_install_mmaction2(&app)
    })?;
    run_step(7, 8, "Building/installing mmengine", cli.debug, || {
        build_and_install_mmengine(&app)
    })?;
    run_step(8, 8, "Running uv sync", true, || run_uv_sync(&app))?;

    println!(
        "{} {}",
        style("✔").green().bold(),
        style("Setup completed successfully.").green().bold()
    );

    Ok(())
}

fn print_header(debug: bool) {
    println!(
        "{} {}",
        style("mmaction-install").cyan().bold(),
        style("(Rust CLI)").dim()
    );
    println!(
        "{} {}",
        style("•").cyan(),
        if debug {
            style("Debug output: enabled").yellow().to_string()
        } else {
            style("Debug output: disabled").dim().to_string()
        }
    );
}

fn run_step<F>(index: usize, total: usize, name: &str, debug: bool, f: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    if debug {
        println!(
            "{} [{index}/{total}] {}",
            style("→").cyan().bold(),
            style(name).cyan()
        );
        return match f() {
            Ok(()) => {
                println!(
                    "{} [{index}/{total}] {}",
                    style("✔").green().bold(),
                    style(name).green()
                );
                Ok(())
            }
            Err(error) => {
                println!(
                    "{} [{index}/{total}] {}",
                    style("✖").red().bold(),
                    style(name).red()
                );
                Err(error).with_context(|| format!("step failed: {name}"))
            }
        };
    }

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("valid spinner template")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner.set_message(format!("[{index}/{total}] {name}"));

    match f() {
        Ok(()) => {
            spinner.finish_with_message(format!(
                "{} [{index}/{total}] {}",
                style("✔").green().bold(),
                style(name).green()
            ));
            Ok(())
        }
        Err(error) => {
            spinner.finish_with_message(format!(
                "{} [{index}/{total}] {}",
                style("✖").red().bold(),
                style(name).red()
            ));
            Err(error).with_context(|| format!("step failed: {name}"))
        }
    }
}

fn check_uv() -> Result<()> {
    let status = Command::new("uv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(exit) if exit.success() => Ok(()),
        _ => bail!(
            "uv is required. Install it from https://docs.astral.sh/uv/getting-started/installation/"
        ),
    }
}

fn ensure_venv(app: &App) -> Result<()> {
    if !Path::new(PYTHON_BIN).exists() {
        let mut command = Command::new("uv");
        command.args(["venv", "--python", "3.12"]);
        run_command(
            app,
            "create virtual environment",
            command,
            OutputMode::Quiet,
        )?;
    }
    Ok(())
}

fn ensure_pip_tooling(app: &App) -> Result<()> {
    let import_status = Command::new(PYTHON_BIN)
        .args(["-c", "import pip"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run python pip import check")?;

    if !import_status.success() {
        let mut command = Command::new("uv");
        command.args([
            "pip",
            "install",
            "--python",
            PYTHON_BIN,
            "pip",
            "setuptools<81",
            "wheel",
        ]);
        run_command(app, "install pip tooling", command, OutputMode::Quiet)?;
    }

    Ok(())
}

fn build_and_install_mmcv(app: &App) -> Result<()> {
    if !wheel_exists("mmcv", MMC_VERSION)? {
        remove_dir_if_exists(".mmcv")?;

        let mut clone = Command::new("git");
        clone.args([
            "clone",
            "--depth",
            "1",
            "--branch",
            &format!("v{MMC_VERSION}"),
            "https://github.com/open-mmlab/mmcv.git",
            ".mmcv",
        ]);
        run_command(app, "clone mmcv", clone, OutputMode::Quiet)?;

        remove_dir_if_exists(".mmcv/.git")?;

        let mut wheel = Command::new(PYTHON_BIN);
        wheel.args([
            "-m",
            "pip",
            "wheel",
            "-v",
            "./.mmcv",
            "--no-deps",
            "--no-build-isolation",
            "--wheel-dir",
            WHEELHOUSE,
        ]);
        run_command(app, "build mmcv wheel", wheel, OutputMode::Quiet)?;
    }

    let mut install = Command::new("uv");
    install.args([
        "pip",
        "install",
        "-v",
        "--python",
        PYTHON_BIN,
        "--no-deps",
        "--no-index",
        "--find-links",
        WHEELHOUSE,
        &format!("mmcv=={MMC_VERSION}"),
    ]);
    run_command(app, "install mmcv", install, OutputMode::Quiet)
}

fn build_and_install_mmaction2(app: &App) -> Result<()> {
    if !wheel_exists("mmaction2", MMACTION_VERSION)? {
        remove_dir_if_exists(".mmaction2")?;

        let mut clone = Command::new("git");
        clone.args([
            "clone",
            "--depth",
            "1",
            "--branch",
            &format!("v{MMACTION_VERSION}"),
            "https://github.com/open-mmlab/mmaction2.git",
            ".mmaction2",
        ]);
        run_command(app, "clone mmaction2", clone, OutputMode::Quiet)?;

        remove_dir_if_exists(".mmaction2/.git")?;

        patch_torch_load_single_line(".mmaction2/mmaction/apis/inference.py")?;
        patch_get_version_function(".mmaction2/setup.py", MMACTION_VERSION)?;

        let mut wheel = Command::new(PYTHON_BIN);
        wheel.args([
            "-m",
            "pip",
            "wheel",
            "-v",
            "./.mmaction2",
            "--no-deps",
            "--no-build-isolation",
            "--wheel-dir",
            WHEELHOUSE,
        ]);
        run_command(app, "build mmaction2 wheel", wheel, OutputMode::Quiet)?;
    }

    let mut install = Command::new("uv");
    install.args([
        "pip",
        "install",
        "-v",
        "--python",
        PYTHON_BIN,
        "--no-deps",
        "--no-index",
        "--find-links",
        WHEELHOUSE,
        &format!("mmaction2=={MMACTION_VERSION}"),
    ]);
    run_command(app, "install mmaction2", install, OutputMode::Quiet)
}

fn build_and_install_mmengine(app: &App) -> Result<()> {
    if !wheel_exists("mmengine", MMENGINE_VERSION)? {
        remove_dir_if_exists(".mmengine")?;

        let mut clone = Command::new("git");
        clone.args([
            "clone",
            "--depth",
            "1",
            "--branch",
            &format!("v{MMENGINE_VERSION}"),
            "https://github.com/open-mmlab/mmengine",
            ".mmengine",
        ]);
        run_command(app, "clone mmengine", clone, OutputMode::Quiet)?;

        remove_dir_if_exists(".mmengine/.git")?;

        patch_get_version_function(".mmengine/setup.py", MMENGINE_VERSION)?;
        patch_torch_load_single_line(".mmengine/mmengine/runner/checkpoint.py")?;

        let mut wheel = Command::new(PYTHON_BIN);
        wheel.args([
            "-m",
            "pip",
            "wheel",
            "-v",
            "./.mmengine",
            "--no-deps",
            "--no-build-isolation",
            "--wheel-dir",
            WHEELHOUSE,
        ]);
        run_command(app, "build mmengine wheel", wheel, OutputMode::Quiet)?;
    }

    let mut install = Command::new("uv");
    install.args([
        "pip",
        "install",
        "-v",
        "--python",
        PYTHON_BIN,
        "--no-deps",
        "--no-index",
        "--find-links",
        WHEELHOUSE,
        &format!("mmengine=={MMENGINE_VERSION}"),
    ]);
    run_command(app, "install mmengine", install, OutputMode::Quiet)
}

fn run_uv_sync(app: &App) -> Result<()> {
    let mut command = Command::new("uv");
    command.arg("sync");
    let mode = if app.debug {
        OutputMode::Stream
    } else {
        OutputMode::Stream
    };
    run_command(app, "uv sync", command, mode)
}

fn run_command(app: &App, label: &str, mut command: Command, mode: OutputMode) -> Result<()> {
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

fn wheel_exists(name: &str, version: &str) -> Result<bool> {
    let pattern = format!("{WHEELHOUSE}/{name}-{version}-*.whl");
    let mut entries = glob(&pattern).with_context(|| format!("invalid glob pattern: {pattern}"))?;
    Ok(entries.next().transpose()?.is_some())
}

fn remove_dir_if_exists(path: &str) -> Result<()> {
    let dir = PathBuf::from(path);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove directory: {}", dir.display()))?;
    }
    Ok(())
}

fn patch_get_version_function(path: &str, version: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("failed reading {path}"))?;
    let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();

    let Some(index) = lines
        .iter()
        .position(|line| line.trim_end() == "def get_version():")
    else {
        return Ok(());
    };

    if lines.len() < index + 4 {
        return Ok(());
    }

    lines.splice(
        index..index + 4,
        [
            "def get_version():".to_string(),
            format!("    return '{version}'"),
        ],
    );

    let mut rewritten = lines.join("\n");
    rewritten.push('\n');
    fs::write(path, rewritten).with_context(|| format!("failed writing {path}"))?;
    Ok(())
}

fn patch_torch_load_single_line(path: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("failed reading {path}"))?;
    let mut replaced_any = false;
    let mut patched = Vec::with_capacity(content.lines().count());

    for line in content.lines() {
        let mut current = line.to_string();
        let mut search_from = 0usize;

        loop {
            let Some(relative_start) = current[search_from..].find("torch.load(") else {
                break;
            };
            let start = search_from + relative_start;
            let open_paren = start + "torch.load".len();
            let rest = &current[open_paren + 1..];
            let Some(close_rel) = rest.find(')') else {
                break;
            };

            let close_idx = open_paren + 1 + close_rel;
            let args = &current[open_paren + 1..close_idx];

            if args.contains("weights_only=") {
                search_from = close_idx + 1;
                continue;
            }

            current.insert_str(close_idx, ", weights_only=False");
            replaced_any = true;
            search_from = close_idx + ", weights_only=False".len() + 1;
        }

        patched.push(current);
    }

    if !replaced_any {
        return Ok(());
    }

    let mut rewritten = patched.join("\n");
    rewritten.push('\n');
    fs::write(path, rewritten).with_context(|| format!("failed writing {path}"))?;

    Ok(())
}
