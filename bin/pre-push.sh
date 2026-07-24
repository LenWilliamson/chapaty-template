#!/usr/bin/env bash
# Pre-push validation. Run locally before pushing:
#   ./bin/pre-push.sh
# CI runs the same checks.
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

cd "$(git rev-parse --show-toplevel)"
echo -e "${BLUE}>>> Starting Local CI Pipeline for chapaty-template...${NC}"

# ==============================================================================
# JOB 1: Compliance & Security
# ==============================================================================

echo -e "\n${YELLOW}[1/10] Checking Security (Secrets)...${NC}"
if git ls-files --error-unmatch .env > /dev/null 2>&1; then
    echo -e "${RED}[FAIL] CRITICAL: .env file is being tracked by git! Remove it immediately.${NC}"
    exit 1
fi
echo -e "${GREEN}[OK] No leaked environment files in git index.${NC}"

echo -e "\n${YELLOW}[2/10] Checking Formatting (Nightly)...${NC}"
# rustfmt.toml relies on nightly-only options (imports_granularity, group_imports,
# wrap_comments, doc_comment_code_block_width, format_macro_bodies), so the format
# check must run on the nightly toolchain or those options are silently ignored.
if ! rustup toolchain list | grep -q nightly \
    || ! rustup component list --toolchain nightly 2>/dev/null | grep -q "rustfmt.*(installed)"; then
    echo -e "${RED}[FAIL] Nightly toolchain with rustfmt is required to enforce rustfmt.toml.${NC}"
    echo "       Please run: rustup toolchain install nightly && rustup component add rustfmt --toolchain nightly"
    exit 1
fi
cargo +nightly fmt --all -- --check || { echo -e "${RED}[FAIL] Formatting invalid. Run 'cargo +nightly fmt --all' to fix.${NC}"; exit 1; }
echo -e "${GREEN}[OK] Formatting is correct.${NC}"

echo -e "\n${YELLOW}[3/10] Checking Cargo.toml Sort Order...${NC}"
if ! command -v cargo-sort &> /dev/null; then
    echo -e "${RED}[FAIL] 'cargo-sort' is not installed.${NC}"
    echo "       Please run: cargo install cargo-sort"
    exit 1
fi
cargo sort --check || { echo -e "${RED}[FAIL] Cargo.toml is unsorted. Run 'cargo sort' to fix.${NC}"; exit 1; }
echo -e "${GREEN}[OK] Cargo.toml dependencies are sorted.${NC}"

echo -e "\n${YELLOW}[4/10] Checking chapaty Dependency (No Local Path)...${NC}"
# Template users must depend on the published crates.io release, not a local
# checkout of the chapaty core lib, or CI/other machines will fail to build.
if grep -E '^chapaty\s*=.*path\s*=' Cargo.toml > /dev/null 2>&1; then
    echo -e "${RED}[FAIL] Cargo.toml contains a local path dependency for chapaty:${NC}"
    grep -E '^chapaty\s*=.*path\s*=' Cargo.toml
    echo "       Replace it with the latest version from crates.io, e.g.:"
    echo "         chapaty = \"<latest-version>\""
    echo "       Check the current release at: https://crates.io/crates/chapaty"
    exit 1
fi
echo -e "${GREEN}[OK] chapaty is pinned to a published release.${NC}"

# ==============================================================================
# JOB 2: Build, Test & Verify
# ==============================================================================

echo -e "\n${YELLOW}[5/10] Security Audit (Dependencies)...${NC}"
if ! command -v cargo-audit &> /dev/null; then
    echo -e "${RED}[FAIL] 'cargo-audit' is not installed.${NC}"
    echo "       Please run: cargo install cargo-audit"
    exit 1
fi
cargo audit
echo -e "${GREEN}[OK] Dependencies audited.${NC}"

echo -e "\n${YELLOW}[6/10] Linting (Clippy)...${NC}"
cargo clippy --all-targets --all-features -- -D warnings
echo -e "${GREEN}[OK] Code is clean.${NC}"

echo -e "\n${YELLOW}[7/10] Building Workspace...${NC}"
cargo build --all-features
echo -e "${GREEN}[OK] Workspace compiled successfully.${NC}"

echo -e "\n${YELLOW}[8/10] Running Unit & Integration Tests...${NC}"
cargo test --all-features --tests
echo -e "${GREEN}[OK] All tests passed.${NC}"

echo -e "\n${YELLOW}[9/10] Verifying Documentation Layout...${NC}"
# Catches broken intra-doc links etc. This is a binary-only crate (no
# src/lib.rs), so there are no doctests to run here -- 'cargo test --doc'
# would error with "no library targets found".
export RUSTDOCFLAGS="-D warnings"
cargo doc --no-deps --document-private-items --all-features
echo -e "${GREEN}[OK] Documentation builds successfully.${NC}"

echo -e "\n${YELLOW}[10/10] Building Release Binary...${NC}"
cargo build --release
echo -e "${GREEN}[OK] Release binary compiled successfully.${NC}"

echo -e "\n${GREEN}>>> SUCCESS! All checks passed. Ready to push.${NC}"
