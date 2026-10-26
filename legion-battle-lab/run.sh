#!/usr/bin/env sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust/Cargo fehlt. Installation: https://www.rust-lang.org/tools/install" >&2
  exit 1
fi
exec cargo run --locked --release -p legion -- "$@"
