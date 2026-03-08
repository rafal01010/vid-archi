#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${REPO_ROOT}"

if [[ ! -f ".env" ]]; then
  echo "Missing .env file in ${REPO_ROOT}. Copy .env.example first."
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is not installed. Install the Rust toolchain before running the backend."
  exit 1
fi

cargo run --manifest-path backend/Cargo.toml
