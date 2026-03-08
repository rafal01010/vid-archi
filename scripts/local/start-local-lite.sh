#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE]"
      exit 1
      ;;
  esac
done

cd "${REPO_ROOT}"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "Missing env file: ${ENV_FILE}"
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is not installed. Install Docker before starting the local packaged services."
  exit 1
fi

echo "Starting MinIO services for local object storage"
docker compose up -d minio minio-bootstrap

"${REPO_ROOT}/scripts/stg/start-stg-app-host.sh" --env-file "${ENV_FILE}"
"${REPO_ROOT}/scripts/stg/start-stg-chunker-host.sh" --env-file "${ENV_FILE}"
"${REPO_ROOT}/scripts/stg/start-stg-transcoder-host.sh" --env-file "${ENV_FILE}"
