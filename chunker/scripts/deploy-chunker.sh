#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

RUN_TESTS=0
OUTPUT_DIR="${REPO_ROOT}/chunker/dist"

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
  echo "cargo is not installed. Install the Rust toolchain before deploying the chunker."
  exit 1
fi

echo "Refreshing chunker Cargo dependencies"
cargo fetch --manifest-path chunker/Cargo.toml

if [[ "${RUN_TESTS}" == "1" ]]; then
  echo "Running chunker Rust tests"
  cargo test --manifest-path chunker/Cargo.toml --offline
fi

echo "Building chunker release binary"
cargo build --release --manifest-path chunker/Cargo.toml --offline

mkdir -p "${OUTPUT_DIR}"
cp "${REPO_ROOT}/chunker/target/release/vid-archi-chunker" "${OUTPUT_DIR}/vid-archi-chunker"
cp "${REPO_ROOT}/config/video_policy.json" "${OUTPUT_DIR}/video_policy.json"
cp "${REPO_ROOT}/.env.example" "${OUTPUT_DIR}/.env.example"

echo "Chunker artifacts prepared in ${OUTPUT_DIR}"
