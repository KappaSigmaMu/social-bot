#!/usr/bin/env sh
set -eu

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-llvm-cov is required.

Install:
  cargo install cargo-llvm-cov

Then rerun:
  ./scripts/coverage.sh
EOF
  exit 127
fi

mkdir -p target/coverage

cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --all-targets --html --output-dir target/coverage
cargo llvm-cov --workspace --all-targets --lcov --output-path target/coverage/lcov.info
cargo llvm-cov report --summary-only
