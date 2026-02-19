# mmaction-install

This is a installer helper for the following libraries in a `uv` python project.

- `mmcv==2.1.0`
- `mmaction2==1.2.0`
- `mmengine==0.10.7`

It builds local wheels into `.wheelhouse`, installs from that local wheelhouse, and runs `uv sync` at the end.

## What it does

The CLI follows the same high-level flow as `setup.sh`:

1. Creates `.wheelhouse` if missing.
2. Ensures `uv` is installed (auto-installs via official installer if missing).
3. Creates `.venv` with Python 3.12 if `.venv/bin/python` does not exist.
4. Ensures `pip`, `setuptools<81`, and `wheel` are available in the venv.
5. For each package (`mmcv`, `mmaction2`, `mmengine`):
   - If a matching wheel is missing in `.wheelhouse`, shallow-clones the tagged repo and builds a wheel.
   - Installs from `.wheelhouse` with `uv pip install --no-index --find-links`.
6. Runs `uv sync`.

## Output behavior

- Default mode (no flags):
  - Shows step progress.
  - Hides command stdout/stderr for all commands **except** final `uv sync`.
  - Shows captured stdout/stderr if a hidden command fails.
- Debug mode (`--debug`):
  - Streams stdout/stderr for all commands.
  - Uses readable step start/success/failure lines (no spinner animation).

## Prerequisites

- Linux/macOS shell environment
- Rust toolchain (`cargo`)
- `curl` or `wget` available on `PATH` (used to auto-install `uv` if missing)
- `git` available on `PATH`
- Network access to clone OpenMMLab repositories

> Note: if `uv` is installed during the run, the installer updates PATH for the current process. If `uv` still cannot be resolved, source your shell rc file (for example, `source ~/.bashrc` or `source ~/.zshrc`) or add `~/.local/bin` to PATH.

## Build

```bash
cargo build --release
```

Binary path:

```bash
target/release/setup
```

## Usage

Run with default (quiet) output:

```bash
./target/release/setup
```

Run with full command output:

```bash
./target/release/setup --debug
```

Purge local cache/repos and force rebuild/reinstall:

```bash
./target/release/setup --purge
```

Use both flags together:

```bash
./target/release/setup --purge --debug
```

Show help:

```bash
./target/release/setup --help
```

`--purge` removes these directories before installation:

- `.wheelhouse`
- `.mmaction2`
- `.mmengine`
- `.mmcv`

## Troubleshooting

- If auto-install cannot run, ensure either `curl` or `wget` is installed.
- If `uv` was installed but still not found, source your shell rc (`~/.bashrc`/`~/.zshrc`) or add `~/.local/bin` to PATH.
- If a command fails in default mode, the CLI prints captured stdout/stderr for that failed command.
- If you need full live logs for everything, rerun with `--debug`.
