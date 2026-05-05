use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::app::App;
use crate::command::{OutputMode, run_command};
use crate::fs_utils::{purge_cache_dirs, remove_dir_if_exists, wheel_exists};
use crate::patches::{patch_get_version_function, patch_torch_load_single_line};
use crate::ui::{print_header, run_step};

const MMC_VERSION: &str = "2.1.0";
const MMACTION_VERSION: &str = "1.2.0";
const MMENGINE_VERSION: &str = "0.10.7";
const WHEELHOUSE: &str = ".wheelhouse";

pub(crate) fn run_setup(app: &App, purge: bool) -> Result<()> {
    let total_steps = if purge { 9 } else { 8 };
    let mut step = 1;

    print_header(app);

    if purge {
        run_step(
            step,
            total_steps,
            "Purging mmaction cache directories",
            app.debug,
            || purge_cache_dirs(WHEELHOUSE),
        )?;
        step += 1;
    }

    run_step(
        step,
        total_steps,
        "Ensuring wheelhouse directory",
        app.debug,
        || fs::create_dir_all(WHEELHOUSE).context("failed to create .wheelhouse directory"),
    )?;
    step += 1;

    run_step(
        step,
        total_steps,
        "Ensuring uv availability",
        app.debug,
        || ensure_uv(app),
    )?;
    step += 1;

    run_step(
        step,
        total_steps,
        "Ensuring Python virtual environment",
        app.debug,
        || ensure_venv(app),
    )?;
    step += 1;

    run_step(step, total_steps, "Ensuring pip tooling", app.debug, || {
        ensure_pip_tooling(app)
    })?;
    step += 1;

    run_step(
        step,
        total_steps,
        "Building/installing mmcv",
        app.debug,
        || build_and_install_mmcv(app),
    )?;
    step += 1;

    run_step(
        step,
        total_steps,
        "Building/installing mmaction2",
        app.debug,
        || build_and_install_mmaction2(app),
    )?;
    step += 1;

    run_step(
        step,
        total_steps,
        "Building/installing mmengine",
        app.debug,
        || build_and_install_mmengine(app),
    )?;
    step += 1;

    run_step(step, total_steps, "Running uv sync", true, || {
        run_uv_sync(app)
    })?;

    println!(
        "{} {}",
        console::style("✔").green().bold(),
        console::style("Setup completed successfully.")
            .green()
            .bold()
    );

    Ok(())
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
    let python_bin = app.python_bin();

    if !python_bin.exists() {
        if let Some(parent) = app.venv_dir.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create parent directory for venv: {}",
                    parent.display()
                )
            })?;
        }

        let mut command = Command::new("uv");
        command
            .arg("venv")
            .arg("--python")
            .arg("3.12")
            .arg(&app.venv_dir);
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
    let python_bin = app.python_bin();

    let import_status = Command::new(&python_bin)
        .args(["-c", "import pip"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run python pip import check")?;

    if !import_status.success() {
        let mut command = Command::new("uv");
        command
            .arg("pip")
            .arg("install")
            .arg("--python")
            .arg(&python_bin)
            .arg("pip")
            .arg("setuptools<81")
            .arg("wheel");
        run_command(app, "install pip tooling", command, OutputMode::Quiet)?;
    }

    Ok(())
}

fn build_and_install_mmcv(app: &App) -> Result<()> {
    if !wheel_exists(WHEELHOUSE, "mmcv", MMC_VERSION)? {
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

        let python_bin = app.python_bin();
        let mut wheel = Command::new(&python_bin);
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
    let python_bin = app.python_bin();
    install
        .arg("pip")
        .arg("install")
        .arg("-v")
        .arg("--python")
        .arg(&python_bin)
        .arg("--no-deps")
        .arg("--no-index")
        .arg("--find-links")
        .arg(WHEELHOUSE)
        .arg(format!("mmcv=={MMC_VERSION}"));
    run_command(app, "install mmcv", install, OutputMode::Quiet)
}

fn build_and_install_mmaction2(app: &App) -> Result<()> {
    if !wheel_exists(WHEELHOUSE, "mmaction2", MMACTION_VERSION)? {
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

        let python_bin = app.python_bin();
        let mut wheel = Command::new(&python_bin);
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
    let python_bin = app.python_bin();
    install
        .arg("pip")
        .arg("install")
        .arg("-v")
        .arg("--python")
        .arg(&python_bin)
        .arg("--no-deps")
        .arg("--no-index")
        .arg("--find-links")
        .arg(WHEELHOUSE)
        .arg(format!("mmaction2=={MMACTION_VERSION}"));
    run_command(app, "install mmaction2", install, OutputMode::Quiet)
}

fn build_and_install_mmengine(app: &App) -> Result<()> {
    if !wheel_exists(WHEELHOUSE, "mmengine", MMENGINE_VERSION)? {
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

        let python_bin = app.python_bin();
        let mut wheel = Command::new(&python_bin);
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
    let python_bin = app.python_bin();
    install
        .arg("pip")
        .arg("install")
        .arg("-v")
        .arg("--python")
        .arg(&python_bin)
        .arg("--no-deps")
        .arg("--no-index")
        .arg("--find-links")
        .arg(WHEELHOUSE)
        .arg(format!("mmengine=={MMENGINE_VERSION}"));
    run_command(app, "install mmengine", install, OutputMode::Quiet)
}

fn run_uv_sync(app: &App) -> Result<()> {
    let mut command = Command::new("uv");
    command.arg("sync");
    let label = if app.venv_was_provided {
        command.arg("--active");
        command.env("VIRTUAL_ENV", &app.venv_dir);
        "uv sync --active"
    } else {
        "uv sync"
    };
    run_command(app, label, command, OutputMode::Stream)
}
