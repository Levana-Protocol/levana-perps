#!/usr/bin/env bash

set -euxo pipefail

SCRIPT=$(readlink -f "$0")
SCRIPTPATH=$(dirname "$SCRIPT")
cd "$SCRIPTPATH"

WASM_DIR="$(pwd)/wasm"
TARGET_CACHE="$WASM_DIR/target"
REGISTRY_CACHE="$WASM_DIR/registry"
CARGO_GIT_CACHE="$WASM_DIR/git"
ARTIFACTS="$WASM_DIR/artifacts"
mkdir -p "$TARGET_CACHE" "$REGISTRY_CACHE" "$ARTIFACTS" "$CARGO_GIT_CACHE"

docker  run --rm --tty \
  -u "$(id -u)":"$(id -g)" \
  -v "$(pwd)":/code \
  -v "$TARGET_CACHE":/target \
  -v "$ARTIFACTS":/code/artifacts \
  -v "$REGISTRY_CACHE":/usr/local/cargo/registry \
  -v "$CARGO_GIT_CACHE":/usr/local/cargo/git \
  cosmwasm/workspace-optimizer:0.15.1
