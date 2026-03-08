#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"
RUN_CHECKS=1
BOOTSTRAP_DB=0
RUN_DB_MIGRATIONS=0
RESET_DB=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    --skip-checks)
      RUN_CHECKS=0
      shift
      ;;
    --bootstrap-db)
      BOOTSTRAP_DB=1
      RUN_DB_MIGRATIONS=1
      shift
      ;;
    --reset-db)
      RESET_DB=1
      BOOTSTRAP_DB=1
      RUN_DB_MIGRATIONS=1
      shift
      ;;
    --migrate-db)
      RUN_DB_MIGRATIONS=1
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--skip-checks] [--bootstrap-db] [--reset-db] [--migrate-db]"
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
  echo "docker is not installed. Install Docker before running the local packaged deploy flow."
  exit 1
fi

echo "Starting MinIO services for local object storage"
docker compose up -d minio minio-bootstrap

DEPLOY_ARGS=(--env-file "${ENV_FILE}")

if [[ "${RUN_CHECKS}" == "1" ]]; then
  DEPLOY_ARGS+=(--run-checks)
fi

if [[ "${BOOTSTRAP_DB}" == "1" ]]; then
  DEPLOY_ARGS+=(--bootstrap-db)
elif [[ "${RUN_DB_MIGRATIONS}" == "1" ]]; then
  DEPLOY_ARGS+=(--migrate-db)
fi

if [[ "${RESET_DB}" == "1" ]]; then
  DEPLOY_ARGS+=(--reset-db)
fi

"${REPO_ROOT}/scripts/stg/deploy-stg-lite.sh" "${DEPLOY_ARGS[@]}"
