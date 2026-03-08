#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

RUN_TESTS=0
OUTPUT_DIR="${REPO_ROOT}/backend/dist"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-tests)
      RUN_TESTS=1
      shift
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--run-tests] [--output-dir DIR]"
      exit 1
      ;;
  esac
done

cd "${REPO_ROOT}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is not installed. Install the Rust toolchain before deploying the backend."
  exit 1
fi

if [[ "${RUN_TESTS}" == "1" ]]; then
  echo "Running Rust unit tests"
  cargo test --manifest-path backend/Cargo.toml
fi

echo "Building release binary"
cargo build --release --manifest-path backend/Cargo.toml

mkdir -p "${OUTPUT_DIR}"
cp "${REPO_ROOT}/backend/target/release/vid-archi-backend" "${OUTPUT_DIR}/vid-archi-backend"
cp "${REPO_ROOT}/config/video_policy.json" "${OUTPUT_DIR}/video_policy.json"
cp "${REPO_ROOT}/.env.example" "${OUTPUT_DIR}/.env.example"

echo "Backend artifacts prepared in ${OUTPUT_DIR}"
echo "Use --run-tests when you want deployment to fail fast before packaging."
