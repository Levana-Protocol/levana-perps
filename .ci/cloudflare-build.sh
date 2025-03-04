#!/usr/bin/env bash

set -euo pipefail

NEW_PATH="$HOME/.cargo/bin"
export PATH="$PATH:$NEW_PATH"

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s --default-toolchain=1.81.0 -- -y
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

cargo binstall just@1.31.0

just build-docs
