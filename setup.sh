#!/usr/bin/env bash

# Fix for mmaction installation: https://github.com/open-mmlab/mmaction2/issues/2714
# Solution: https://github.com/open-mmlab/mmaction2/issues/2714#issuecomment-1816537184

# Fix by downloading https://github.com/open-mmlab/mmaction2/tree/4d6c93474730cad2f25e51109adcf96824efc7a3/mmaction/models/localizers/drn folder
# or by installing mmaction2 manually, as we are doing below.

MMC_VERSION="2.1.0"
MMACTION_VERSION="1.2.0"
MMENGINE_VERSION="0.10.7"
WHEELHOUSE=".wheelhouse"

mkdir -p "$WHEELHOUSE"

# Here we manually install mmaction2 to make sure its installed fully.
set -euo pipefail

if ! command -v uv >/dev/null 2>&1; then
	echo "uv is required. Install it from https://docs.astral.sh/uv/getting-started/installation/"
	exit 1
fi

if [ ! -f ".venv/bin/python" ]; then
	uv venv --python 3.12
fi

PYTHON_BIN=".venv/bin/python"

if ! "$PYTHON_BIN" -c "import pip" >/dev/null 2>&1; then
	uv pip install --python "$PYTHON_BIN" pip "setuptools<81" wheel
fi

if ! ls "$WHEELHOUSE"/mmcv-"${MMC_VERSION}"-*.whl >/dev/null 2>&1; then
	rm -rf .mmcv
	git clone --depth 1 --branch "v${MMC_VERSION}" https://github.com/open-mmlab/mmcv.git .mmcv
	rm -rf .mmcv/.git
	"$PYTHON_BIN" -m pip wheel -v ./.mmcv --no-deps --no-build-isolation --wheel-dir "$WHEELHOUSE"
fi
uv pip install -v --python "$PYTHON_BIN" --no-deps --no-index --find-links "$WHEELHOUSE" "mmcv==${MMC_VERSION}"

if ! ls "$WHEELHOUSE"/mmaction2-"${MMACTION_VERSION}"-*.whl >/dev/null 2>&1; then
	rm -rf .mmaction2
	git clone --depth 1 --branch "v${MMACTION_VERSION}" https://github.com/open-mmlab/mmaction2.git .mmaction2
	rm -rf .mmaction2/.git
	sed -i 's/torch\.load(\([^)]*\))/torch.load(\1, weights_only=False)/g' .mmaction2/mmaction/apis/inference.py
	sed -i '/^def get_version():$/,+3{/^def get_version():$/!d; a\    return '\''1.2.0'\''
}' .mmaction2/setup.py
	"$PYTHON_BIN" -m pip wheel -v ./.mmaction2 --no-deps --no-build-isolation --wheel-dir "$WHEELHOUSE"
fi
uv pip install -v --python "$PYTHON_BIN" --no-deps --no-index --find-links "$WHEELHOUSE" "mmaction2==${MMACTION_VERSION}"

if ! ls "$WHEELHOUSE"/mmengine-"${MMENGINE_VERSION}"-*.whl >/dev/null 2>&1; then
	rm -rf .mmengine
	git clone --depth 1 --branch "v${MMENGINE_VERSION}" https://github.com/open-mmlab/mmengine .mmengine
	rm -rf .mmengine/.git
	sed -i '/^def get_version():$/,+3{/^def get_version():$/!d; a\    return '\''0.10.7'\''
}' .mmengine/setup.py
	sed -i 's/torch\.load(\([^)]*\))/torch.load(\1, weights_only=False)/g' .mmengine/mmengine/runner/checkpoint.py
	"$PYTHON_BIN" -m pip wheel -v ./.mmengine --no-deps --no-build-isolation --wheel-dir "$WHEELHOUSE"
fi
uv pip install -v --python "$PYTHON_BIN" --no-deps --no-index --find-links "$WHEELHOUSE" "mmengine==${MMENGINE_VERSION}"

uv sync