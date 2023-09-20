#!/usr/bin/env bash

set -euxo pipefail

SCRIPT=$(readlink -f "$0")
SCRIPTPATH=$(dirname "$SCRIPT")
cd "$SCRIPTPATH"
cd ..

WASM_DIR="$(pwd)/wasm"
TARGET_CACHE="$WASM_DIR/target"
REGISTRY_CACHE="$WASM_DIR/registry"
CARGO_GIT_CACHE="$WASM_DIR/git"
ARTIFACTS="$WASM_DIR/artifacts"

if [[ -n "${OPTIMIZER_ARM64:-}" ]]; then
    echo "sed on OSX is weird. Use gsed instead"
    SED=gsed
else
    SED=sed
fi

if [ -n "${SEI:-}" ]; then
    echo "If this script failed, it would left extra \`default = [\"sei\"]\` line in contracts' Cargo.toml."
    
    for i in contracts/market/; do
        grep -q '^default = \["sei"\]$' "${i}/Cargo.toml" || $SED -i -e '/\[features\]/ a default = ["sei"]' "${i}/Cargo.toml"
    done
fi

if [[ -n "${OPTIMIZER_ARM64:-}" ]]; then
    OPTIMIZER_VERSION="cosmwasm/workspace-optimizer-arm64":0.14.0
else
    OPTIMIZER_VERSION="cosmwasm/workspace-optimizer":0.14.0
fi

mkdir -p "$TARGET_CACHE" "$REGISTRY_CACHE" "$ARTIFACTS" "$CARGO_GIT_CACHE"

# Delete the old file to avoid false positives if the compilation fails
rm -f "$WASM_DIR/artifacts/gitrev"

docker  run --rm --tty \
-u "$(id -u)":"$(id -g)" \
-v "$(pwd)":/code \
-v "$TARGET_CACHE":/target \
-v "$ARTIFACTS":/code/artifacts \
-v "$REGISTRY_CACHE":/usr/local/cargo/registry \
-v "$CARGO_GIT_CACHE":/usr/local/cargo/git \
$OPTIMIZER_VERSION

if [ -n "${SEI:-}" ]; then
    for i in "${ARTIFACTS}/"*market*; do
        mv "${i}" "${i%.wasm}-sei.wasm"
    done
fi

# not sure how this was created since we mapped the tool's /code/artifacts
# but it's empty (the real artifacts are in wasm/artifacts)
rm -rf ./artifacts

# Only write the gitrev file on success
git rev-parse HEAD > "$WASM_DIR/artifacts/gitrev"

if [ -n "${SEI:-}" ]; then
    for i in contracts/market/; do
        $SED -i -e '/default = \["sei"\]/ d' "${i}/Cargo.toml"
    done
fi
