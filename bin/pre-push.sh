#!/usr/bin/env bash
# Pre-push validation. Run locally before pushing:
#   ./bin/pre-push.sh
# CI runs the same checks.
set -euo pipefail

echo ">> [0/4] Checking Cargo.toml for local path dependency on chapaty..."
if grep -E '^chapaty\s*=.*path\s*=' Cargo.toml > /dev/null 2>&1; then
  echo ""
  echo "ERROR: Cargo.toml contains a local path dependency for chapaty:"
  echo ""
  grep -E '^chapaty\s*=.*path\s*=' Cargo.toml
  echo ""
  echo "  Replace it with the latest version from crates.io, e.g.:"
  echo "    chapaty = \"<latest-version>\""
  echo ""
  echo "  Check the current release at: https://crates.io/crates/chapaty"
  echo ""
  exit 1
fi

echo ">> [1/5] cargo fmt --check"
cargo fmt --all -- --check

echo ">> [2/5] cargo clippy -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

echo ">> [3/5] cargo audit"
if ! command -v cargo-audit > /dev/null 2>&1; then
  echo "  cargo-audit not found, installing..."
  cargo install cargo-audit --quiet
fi
cargo audit

echo ">> [4/5] cargo test"
cargo test --all-features

echo ">> [5/5] cargo build --release"
cargo build --release

echo ">> All checks passed."
