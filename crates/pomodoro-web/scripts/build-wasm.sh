#!/bin/bash
set -e

# Build the WebAssembly module and generate the JavaScript bindings.
#
# Usage:
#   ./scripts/build-wasm.sh            # debug build
#   ./scripts/build-wasm.sh --release  # release build

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$SCRIPT_DIR/.."
WORKSPACE_ROOT="$PROJECT_ROOT/../.."

RELEASE_FLAG=""
if [[ "$1" == "--release" ]]; then
    RELEASE_FLAG="--release"
fi

echo "==> Building WASM (${RELEASE_FLAG:-debug})"
cd "$WORKSPACE_ROOT"
cargo rustc -p pomodoro-web --target wasm32-unknown-unknown $RELEASE_FLAG -- \
    -C link-arg=-zstack-size=8388608

if [[ -n "$RELEASE_FLAG" ]]; then
    BUILD_MODE="release"
else
    BUILD_MODE="debug"
fi

WASM_PATH="$WORKSPACE_ROOT/target/wasm32-unknown-unknown/$BUILD_MODE/pomodoro_web.wasm"
if [[ ! -f "$WASM_PATH" ]]; then
    echo "Error: WASM file not found at: $WASM_PATH" >&2
    exit 1
fi

echo "==> Generating JavaScript bindings"
wasm-bindgen "$WASM_PATH" \
    --out-dir "$PROJECT_ROOT/www/src/wasm" \
    --target web \
    --no-typescript

echo "==> Done. Run 'cd www && bun install && bun run dev' to serve it."
