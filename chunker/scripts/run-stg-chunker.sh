#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
DIST_DIR="${REPO_ROOT}/chunker/dist"
LOG_DIR="${REPO_ROOT}/logs"
BACKGROUND=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    --dist-dir)
      DIST_DIR="$2"
      shift 2
      ;;
    --background)
      BACKGROUND=1
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--dist-dir DIR] [--background]"
      exit 1
      ;;
  esac
done

if [[ -f "${ENV_FILE}" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "${ENV_FILE}"
  set +a
fi

cd "${REPO_ROOT}"

if [[ ! -x "${DIST_DIR}/vid-archi-chunker" ]]; then
  echo "Missing chunker binary at ${DIST_DIR}/vid-archi-chunker. Run ./chunker/scripts/deploy-chunker.sh first."
  exit 1
fi

export DATABASE_URL="${STG_DATABASE_URL:-${DATABASE_URL:-}}"
export RUST_LOG="${RUST_LOG:-info}"

if [[ -z "${DATABASE_URL}" ]]; then
  echo "DATABASE_URL is not set. Provide STG_DATABASE_URL or DATABASE_URL in the env file."
  exit 1
fi

mkdir -p "${LOG_DIR}"

if [[ "${BACKGROUND}" == "1" ]]; then
  echo "Starting chunker in background with RUST_LOG=${RUST_LOG}"
  nohup "${DIST_DIR}/vid-archi-chunker" > "${LOG_DIR}/chunker.log" 2>&1 &
  echo $! > "${LOG_DIR}/chunker.pid"
  echo "Chunker PID $(cat "${LOG_DIR}/chunker.pid")"
else
  exec "${DIST_DIR}/vid-archi-chunker"
fi
