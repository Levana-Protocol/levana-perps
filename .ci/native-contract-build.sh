#!/usr/bin/env bash

# native build mimics the same commands as the docker tool
# however, it wipes the directory and *always* re-builds
# prerequisites must be installed first:
# rust with wasm32-unknown-unknown target
# wasm-opt available on the path somewhere
# sha256sum

set -euxo pipefail

SCRIPT=$(readlink -f "$0")
SCRIPTPATH=$(dirname "$SCRIPT")
cd "$SCRIPTPATH"
cd ..

WASM_DIR="$(pwd)/wasm"
CONTRACTS_DIR="$(pwd)/contracts"
RELEASE_DIR="$(pwd)/target/wasm32-unknown-unknown/release"
INTERMEDIATE_SHAS="$WASM_DIR/artifacts/checksums_intermediate.txt"
ARTIFACTS="$WASM_DIR/artifacts"

rm -rf "$WASM_DIR"
mkdir -p "$ARTIFACTS"

for CONTRACT_PATH in "$CONTRACTS_DIR"/*; do
CONTRACT_NAME=$(basename "$CONTRACT_PATH")
WASM="$RELEASE_DIR/levana_perpswap_cosmos_$CONTRACT_NAME.wasm"

echo "BUILDING $CONTRACT_NAME"
cd "$CONTRACT_PATH"

RUSTFLAGS="-C link-arg=-s" cargo build --release --lib --target=wasm32-unknown-unknown

INTERMEDIATE_SHA=$(sha256sum -- "$WASM" | sed 's,../target,target,g')
echo "$INTERMEDIATE_SHA" >>"$INTERMEDIATE_SHAS"

OPTIMIZED_WASM="$WASM_DIR/artifacts/levana_perpswap_cosmos_$CONTRACT_NAME.wasm"
$WASM_OPT -Os "$WASM" -o "$OPTIMIZED_WASM"
done

cd "$WASM_DIR/artifacts"
sha256sum -- *.wasm | tee checksums.txt

# Only write the gitrev file on success
git rev-parse HEAD > "$WASM_DIR/artifacts/gitrev"
