#!/usr/bin/env bash
# Mirrors .github/workflows/vanilla.yml locally.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo run --release -p mcrust-cli -- setup --version 26.3 --assets index
cargo run --release -p mcrust-cli -- info --version 26.3

root="${MCRUST_HOME:-$HOME/.mcrust}/versions/26.3/resources/assets/minecraft"
test -f "$root/textures/block/stone.png"
test -f "$root/models/block/stone.json"
test -f "$root/blockstates/grass_block.json"
test -f "${MCRUST_HOME:-$HOME/.mcrust}/assets/indexes/34.json"
echo "vanilla pipeline OK"
