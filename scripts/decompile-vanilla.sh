#!/usr/bin/env bash
# Decompiles the official 26.3 client into ~/.mcrust/vanilla/26.3/src.
# The output is Mojang's code: it stays local and must never be committed.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --release -p mcrust-cli -- decompile "$@"
