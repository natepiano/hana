#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

export RUSTC_WRAPPER=

echo "==> cargo +nightly fmt --all --check"
cargo +nightly fmt --all --check

if command -v taplo >/dev/null 2>&1; then
  echo "==> taplo fmt --check"
  taplo fmt --check
else
  echo "==> taplo fmt --check (skipped: taplo not installed)"
fi

echo "==> cargo clippy --all-targets --all-features"
cargo clippy --all-targets --all-features

echo "==> cargo build --release --all-targets"
cargo build --release --all-targets

echo "==> cargo bench --bench layout_perf -- --noplot"
cargo bench --bench layout_perf -- --noplot

echo "==> cargo nextest run --all-features"
CARGO_TERM_COLOR=never cargo nextest run --all-features

echo "==> cargo mend"
CARGO_TERM_COLOR=never cargo mend
