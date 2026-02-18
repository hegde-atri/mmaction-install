# mmaction-install

Rust CLI installer that replicates the behavior of `setup.sh` for installing:

- `mmcv==2.1.0`
- `mmaction2==1.2.0`
- `mmengine==0.10.7`

It builds local wheels into `.wheelhouse`, installs from that local wheelhouse, and runs `uv sync` at the end.

## What it does

The CLI follows the same high-level flow as `setup.sh`:

1. Creates `.wheelhouse` if missing.
2. Verifies `uv` is installed.
3. Creates `.venv` with Python 3.12 if `.venv/bin/python` does not exist.
4. Ensures `pip`, `setuptools<81`, and `wheel` are available in the venv.
5. For each package (`mmcv`, `mmaction2`, `mmengine`):
   - If a matching wheel is missing in `.wheelhouse`, shallow-clones the tagged repo and builds a wheel.
   - Installs from `.wheelhouse` with `uv pip install --no-index --find-links`.
6. Runs `uv sync`.

## Output behavior

- Default mode (no flags):
  - Shows pretty step progress (spinner + colored status).
  - Hides command stdout/stderr for all commands **except** `uv sync`.
  - Shows captured stdout/stderr if a hidden command fails.
- Debug mode (`--debug`):
  - Streams stdout/stderr for all commands.
  - Uses readable step start/success/failure lines (no spinner animation).

## Prerequisites

- Linux/macOS shell environment
- Rust toolchain (`cargo`)
- `uv` available on `PATH`
- `git` available on `PATH`
- Network access to clone OpenMMLab repositories

> Note: the installer explicitly checks `uv`. Other missing tools will fail when their command is reached.

## Build

```bash
cargo build --release
```

Binary path:

```bash
target/release/mmaction-install
```

## Usage

Run with default (quiet) output:

```bash
./target/release/mmaction-install
```

Run with full command output:

```bash
./target/release/mmaction-install --debug
```

Purge local cache/repos and force rebuild/reinstall:

```bash
./target/release/mmaction-install --purge
```

Use both flags together:

```bash
./target/release/mmaction-install --purge --debug
```

Show help:

```bash
./target/release/mmaction-install --help
```

`--purge` removes these directories before installation:

- `.wheelhouse`
- `.mmaction2`
- `.mmengine`
- `.mmcv`

## Notes on parity with `setup.sh`

- Uses the same package versions and installation order.
- Uses local wheelhouse checks before cloning/building.
- Applies source patches needed for `mmaction2`/`mmengine` wheel builds.
- Keeps final `uv sync` output visible in both default and debug modes.

## Troubleshooting

- If you see `uv is required...`, install `uv` first:
  - https://docs.astral.sh/uv/getting-started/installation/
- If a command fails in default mode, the CLI prints captured stdout/stderr for that failed command.
- If you need full live logs for everything, rerun with `--debug`.
