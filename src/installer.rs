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

pub(crate) fn run_setup(app: &App, purge: bool, skip_pre_commit_install: bool) -> Result<()> {
    let total_steps = total_setup_steps(purge, skip_pre_commit_install);
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

    let mut env_file_status = None;
    run_step(
        step,
        total_steps,
        "Ensuring project .env file",
        app.debug,
        || {
            env_file_status = Some(ensure_project_env_file(".")?);
            Ok(())
        },
    )?;
    print_env_file_notice(env_file_status.expect(".env step stores a status"));
    step += 1;

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
    step += 1;

    if !skip_pre_commit_install {
        run_step(
            step,
            total_steps,
            "Installing pre-commit hook",
            app.debug,
            || install_pre_commit_hook(app),
        )?;
    }

    println!(
        "{} {}",
        console::style("✔").green().bold(),
        console::style("Setup completed successfully.")
            .green()
            .bold()
    );

    Ok(())
}

fn total_setup_steps(purge: bool, skip_pre_commit_install: bool) -> usize {
    9 + usize::from(purge) + usize::from(!skip_pre_commit_install)
}

#[derive(Debug, PartialEq, Eq)]
enum EnvFileStatus {
    Copied,
    Skipped,
}

fn ensure_project_env_file(project_dir: impl AsRef<Path>) -> Result<EnvFileStatus> {
    let project_dir = project_dir.as_ref();
    let env_file = project_dir.join(".env");

    if env_file.exists() {
        return Ok(EnvFileStatus::Skipped);
    }

    let env_example = project_dir.join(".env.example");
    fs::copy(&env_example, &env_file).with_context(|| {
        format!(
            "failed to copy {} to {}",
            env_example.display(),
            env_file.display()
        )
    })?;

    Ok(EnvFileStatus::Copied)
}

fn print_env_file_notice(status: EnvFileStatus) {
    let border = console::style("!".repeat(72)).yellow().bold();
    println!();
    println!("{border}");
    match status {
        EnvFileStatus::Copied => {
            println!(
                "{} {}",
                console::style("Environment file created:").yellow().bold(),
                console::style("copied .env.example to .env").yellow()
            );
            println!(
                "{}",
                console::style(
                    "Review .env and change values as necessary before running the app."
                )
                .yellow()
            );
        }
        EnvFileStatus::Skipped => {
            println!(
                "{} {}",
                console::style("Environment file step skipped:")
                    .dim()
                    .bold(),
                console::style(".env already exists").dim()
            );
        }
    }
    println!("{border}");
    println!();
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

fn install_pre_commit_hook(app: &App) -> Result<()> {
    run_command(
        app,
        "uv tool run pre-commit install",
        pre_commit_install_command(),
        OutputMode::Quiet,
    )
    .map_err(pre_commit_install_error_context)
}

fn pre_commit_install_error_context(error: anyhow::Error) -> anyhow::Error {
    error.context("failed to install pre-commit hook, run make pre-commit-install")
}

fn pre_commit_install_command() -> Command {
    let mut command = Command::new("uv");
    command.args(["tool", "run", "pre-commit", "install"]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "mmaction-install-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp project dir");
        dir
    }

    #[test]
    fn builds_direct_pre_commit_install_command() {
        let command = pre_commit_install_command();

        assert_eq!(command.get_program(), "uv");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, ["tool", "run", "pre-commit", "install"]);
    }

    #[test]
    fn pre_commit_install_failure_context_includes_make_target_hint() {
        let error = pre_commit_install_error_context(anyhow::anyhow!("pre-commit failed"));

        assert_eq!(
            format!("{error:#}"),
            "failed to install pre-commit hook, run make pre-commit-install: pre-commit failed"
        );
    }

    #[test]
    fn setup_steps_exclude_pre_commit_install_when_skipped() {
        assert_eq!(total_setup_steps(false, false), 10);
        assert_eq!(total_setup_steps(false, true), 9);
        assert_eq!(total_setup_steps(true, false), 11);
        assert_eq!(total_setup_steps(true, true), 10);
    }

    #[test]
    fn copies_env_example_when_env_is_missing() {
        let project_dir = temp_project_dir("copy-env");
        fs::write(project_dir.join(".env.example"), "TOKEN=change-me\n")
            .expect("write env example");

        let result = ensure_project_env_file(&project_dir).expect("ensure .env file");

        assert_eq!(result, EnvFileStatus::Copied);
        assert_eq!(
            fs::read_to_string(project_dir.join(".env")).expect("read copied env"),
            "TOKEN=change-me\n"
        );

        fs::remove_dir_all(project_dir).expect("cleanup temp project dir");
    }

    #[test]
    fn skips_env_copy_when_env_already_exists() {
        let project_dir = temp_project_dir("skip-env");
        fs::write(project_dir.join(".env.example"), "TOKEN=from-example\n")
            .expect("write env example");
        fs::write(project_dir.join(".env"), "TOKEN=existing\n").expect("write env");

        let result = ensure_project_env_file(&project_dir).expect("ensure .env file");

        assert_eq!(result, EnvFileStatus::Skipped);
        assert_eq!(
            fs::read_to_string(project_dir.join(".env")).expect("read env"),
            "TOKEN=existing\n"
        );

        fs::remove_dir_all(project_dir).expect("cleanup temp project dir");
    }
}
