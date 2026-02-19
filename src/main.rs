use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
#[command(name = "setup", author, version, about = "Install mmaction stack with local wheel builds and run uv sync")]
struct Cli {
    #[arg(long, default_value_t = false, help = "Show command output while running setup")]
    debug: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Delete .wheelhouse, .mmaction2, .mmengine, and .mmcv before reinstalling"
    )]
    purge: bool,
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
    let total_steps = if cli.purge { 9 } else { 8 };
    let mut step = 1;

    print_header(cli.debug);

    if cli.purge {
        run_step(step, total_steps, "Purging mmaction cache directories", cli.debug, || {
            purge_cache_dirs()
        })?;
        step += 1;
    }

    run_step(step, total_steps, "Ensuring wheelhouse directory", cli.debug, || {
        fs::create_dir_all(WHEELHOUSE).context("failed to create .wheelhouse directory")
    })?;
    step += 1;

    run_step(step, total_steps, "Ensuring uv availability", cli.debug, || {
        ensure_uv(&app)
    })?;
    step += 1;

    run_step(step, total_steps, "Ensuring Python virtual environment", cli.debug, || {
        ensure_venv(&app)
    })?;
    step += 1;

    run_step(step, total_steps, "Ensuring pip tooling", cli.debug, || ensure_pip_tooling(&app))?;
    step += 1;

    run_step(step, total_steps, "Building/installing mmcv", cli.debug, || {
        build_and_install_mmcv(&app)
    })?;
    step += 1;

    run_step(step, total_steps, "Building/installing mmaction2", cli.debug, || {
        build_and_install_mmaction2(&app)
    })?;
    step += 1;

    run_step(step, total_steps, "Building/installing mmengine", cli.debug, || {
        build_and_install_mmengine(&app)
    })?;
    step += 1;

    run_step(step, total_steps, "Running uv sync", true, || run_uv_sync(&app))?;

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
        style("./setup").cyan().bold(),
        style("CLI that installs mmaction stack with local wheel builds and runs uv sync").dim()
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
                Err(error).with_context(|| format!("step failed: {name}"))
            }
        };
    }

    let tick_set = &[
        "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
    ];

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan.bold} {prefix:.dim} {msg} {elapsed_precise:.dim}")
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
            Err(error).with_context(|| format!("step failed: {name}"))
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

fn ensure_uv(app: &App) -> Result<()> {
    if uv_is_available() {
        return Ok(());
    }

    for candidate_dir in uv_candidate_dirs() {
        if candidate_dir.join("uv").exists() {
            prepend_path_dir(&candidate_dir)?;
            if uv_is_available() {
                return Ok(());
            }
        }
    }

    if command_exists("curl") {
        let mut command = Command::new("sh");
        command.args(["-c", "curl -LsSf https://astral.sh/uv/install.sh | sh"]);
        run_command(app, "install uv", command, OutputMode::Quiet)?;
    } else if command_exists("wget") {
        let mut command = Command::new("sh");
        command.args(["-c", "wget -qO- https://astral.sh/uv/install.sh | sh"]);
        run_command(app, "install uv", command, OutputMode::Quiet)?;
    } else {
        bail!(
            "uv is missing and cannot be auto-installed because neither curl nor wget is available"
        );
    }

    for candidate_dir in uv_candidate_dirs() {
        if candidate_dir.join("uv").exists() {
            prepend_path_dir(&candidate_dir)?;
        }
    }

    if uv_is_available() {
        return Ok(());
    }

    bail!(
        "uv installation completed but `uv` is still not on PATH. Try sourcing your shell rc (for example `source ~/.bashrc` or `source ~/.zshrc`) or add ~/.local/bin to PATH"
    );
}

fn uv_is_available() -> bool {
    Command::new("uv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_exists(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&path_var).any(|dir| {
        let candidate = dir.join(name);
        if !candidate.is_file() {
            return false;
        }
        fs::metadata(candidate)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

fn uv_candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = std::env::var_os("HOME") {
        let home_path = PathBuf::from(home);
        dirs.push(home_path.join(".local/bin"));
        dirs.push(home_path.join(".cargo/bin"));
    }

    dirs
}

fn prepend_path_dir(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    let existing = std::env::var_os("PATH").unwrap_or_default();
    let already_present = std::env::split_paths(&existing).any(|path| path == dir);
    if already_present {
        return Ok(());
    }

    let mut updated_paths = Vec::new();
    updated_paths.push(dir.to_path_buf());
    updated_paths.extend(std::env::split_paths(&existing));
    let joined = std::env::join_paths(updated_paths).context("failed to build updated PATH")?;

    unsafe {
        std::env::set_var("PATH", &joined);
    }

    Ok(())
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

fn purge_cache_dirs() -> Result<()> {
    for path in [WHEELHOUSE, ".mmaction2", ".mmengine", ".mmcv"] {
        remove_dir_if_exists(path)?;
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
