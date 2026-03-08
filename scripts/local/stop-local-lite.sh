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

if [[ -f "${ENV_FILE}" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "${ENV_FILE}"
  set +a
fi

BACKEND_PID_FILE="${REPO_ROOT}/logs/backend.pid"
FRONTEND_PID_FILE="${REPO_ROOT}/logs/frontend.pid"
POSTGRES_CONTAINER_NAME="${STG_POSTGRES_CONTAINER_NAME:-local-lite-postgres}"

stop_pid_file() {
  local pid_file="$1"

  if [[ ! -f "${pid_file}" ]]; then
    return 0
  fi

  local pid
  pid="$(cat "${pid_file}")"

  if kill -0 "${pid}" >/dev/null 2>&1; then
    kill "${pid}" >/dev/null 2>&1 || true
  fi

  rm -f "${pid_file}"
}

echo "Stopping packaged backend/frontend services"
stop_pid_file "${BACKEND_PID_FILE}"
stop_pid_file "${FRONTEND_PID_FILE}"

if command -v docker >/dev/null 2>&1; then
  echo "Stopping local MinIO services"
  docker compose stop minio minio-bootstrap >/dev/null 2>&1 || true

  if docker container inspect "${POSTGRES_CONTAINER_NAME}" >/dev/null 2>&1; then
    echo "Stopping local Postgres container ${POSTGRES_CONTAINER_NAME}"
    docker stop "${POSTGRES_CONTAINER_NAME}" >/dev/null 2>&1 || true
  fi
fi

echo "Local packaged services stopped."
