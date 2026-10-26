#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo test --workspace --exclude terra-demo --exclude terra-bevy --locked
cargo test -p terra-bevy --no-default-features --locked
cargo check -p terra-demo --locked
cargo test -p terra-bevy --lib --locked
cargo test -p terra-demo --locked
cargo run -p terra-bevy --no-default-features --example headless --locked
cargo run -p terra-bevy --no-default-features --example custom_provider --locked
