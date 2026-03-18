#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_FILE="$(readlink -f "$0")"
SCRIPT_DIR="$(dirname "${SCRIPT_FILE}")"
MODULE_DIR="$(dirname "${SCRIPT_DIR}")"

cd "${MODULE_DIR}"
cargo test --workspace --all-features --lib --bins --tests --exclude rivet-plugin-sdk --exclude rivet-python
cargo test -p rivet-python --lib
cargo test -p rivet-plugin-sdk --all-features --lib --tests
