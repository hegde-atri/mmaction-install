# CLAUDE.md

## Project Overview

`mmaction-install` builds a Rust CLI binary named `setup`. The CLI prepares a Python `uv` environment for an OpenMMLab mmaction stack by building local wheels for:

- `mmcv==2.1.0`
- `mmaction2==1.2.0`
- `mmengine==0.10.7`

The installer stores wheels in `.wheelhouse`, uses temporary local clones under `.mmcv`, `.mmaction2`, and `.mmengine`, installs from the local wheelhouse, then runs `uv sync`.

## Common Commands

- `cargo fmt --check` - verify Rust formatting.
- `cargo check` - compile-check the project quickly.
- `cargo build --release` - build the default release binary.
- `make build` - build a static Linux x86-64 musl binary.
- `./target/release/setup --help` - inspect CLI flags.

Avoid running the full installer as a routine verification step because it can clone repositories, build wheels, install packages, and run `uv sync`.

## Source Layout

- `src/main.rs` - thin entrypoint: parse CLI args, build `App`, run setup.
- `src/cli.rs` - clap argument definitions.
- `src/app.rs` - runtime app configuration and virtualenv path resolution.
- `src/installer.rs` - high-level setup flow and package-specific build/install steps.
- `src/command.rs` - command execution and quiet/streaming output behavior.
- `src/ui.rs` - setup header, progress steps, and elapsed-time formatting.
- `src/fs_utils.rs` - wheelhouse lookup and cache directory cleanup helpers.
- `src/patches.rs` - source patch helpers applied before wheel builds.

## Development Notes

- Keep `main.rs` small. Add behavior to the module that owns the responsibility.
- Preserve the quiet default output behavior: hidden command logs should be printed only when a hidden command fails, except final `uv sync`, which streams.
- Keep package versions and wheelhouse behavior centralized in `installer.rs`.
- Source patch helpers should be idempotent when possible because rebuilt directories may already contain patched content.
- Prefer focused helpers over broad abstractions; this is a small installer, not a framework.

## Safety Notes

- `--purge` removes `.wheelhouse`, `.mmaction2`, `.mmengine`, and `.mmcv`.
- The installer may auto-install `uv` using the official installer when `uv` is missing and `curl` or `wget` is available.
- `--venv` can point outside the repository. Relative paths are resolved from the current working directory.
