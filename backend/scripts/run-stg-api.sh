#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
DIST_DIR="${REPO_ROOT}/backend/dist"
LOG_DIR="${REPO_ROOT}/logs"
BACKGROUND=0
PID_FILE="${LOG_DIR}/backend.pid"

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

if [[ ! -x "${DIST_DIR}/vid-archi-backend" ]]; then
  echo "Missing backend binary at ${DIST_DIR}/vid-archi-backend. Run ./backend/scripts/deploy-backend.sh first."
  exit 1
fi

export DATABASE_URL="${STG_DATABASE_URL:-${DATABASE_URL:-}}"
export RUST_LOG="${RUST_LOG:-info}"

if [[ -z "${DATABASE_URL}" ]]; then
  echo "DATABASE_URL is not set. Provide STG_DATABASE_URL or DATABASE_URL in the env file."
  exit 1
fi

mkdir -p "${LOG_DIR}"

stop_existing_backend() {
  if [[ ! -f "${PID_FILE}" ]]; then
    return
  fi

  local existing_pid
  existing_pid="$(cat "${PID_FILE}")"

  if [[ -n "${existing_pid}" ]] && kill -0 "${existing_pid}" >/dev/null 2>&1; then
    echo "Stopping existing backend process ${existing_pid}"
    kill "${existing_pid}" >/dev/null 2>&1 || true
    wait "${existing_pid}" 2>/dev/null || true
  fi

  rm -f "${PID_FILE}"
}

find_listening_pid() {
  local port="$1"

  if command -v lsof >/dev/null 2>&1; then
    lsof -t -nP -iTCP:"${port}" -sTCP:LISTEN 2>/dev/null | head -n1
    return
  fi

  if command -v ss >/dev/null 2>&1; then
    ss -ltnp "( sport = :${port} )" 2>/dev/null \
      | grep -o 'pid=[0-9]\+' \
      | head -n1 \
      | cut -d= -f2
  fi
}

stop_port_listener() {
  local port="$1"
  local label="$2"
  local listener_pid

  listener_pid="$(find_listening_pid "${port}")"

  if [[ -z "${listener_pid}" ]]; then
    return
  fi

  echo "Stopping ${label} port listener ${listener_pid} on ${port}"
  kill "${listener_pid}" >/dev/null 2>&1 || true
  wait "${listener_pid}" 2>/dev/null || true
  sleep 1

  listener_pid="$(find_listening_pid "${port}")"
  if [[ -n "${listener_pid}" ]]; then
    echo "Force stopping ${label} port listener ${listener_pid} on ${port}"
    kill -9 "${listener_pid}" >/dev/null 2>&1 || true
    sleep 1
  fi
}

stop_existing_backend
stop_port_listener "${API_PORT:-8080}" "backend"

if [[ -n "$(find_listening_pid "${API_PORT:-8080}")" ]]; then
  echo "Backend port ${API_PORT:-8080} is still in use after restart cleanup."
  echo "Stop the conflicting process before starting the STG backend."
  exit 1
fi

if [[ "${BACKGROUND}" == "1" ]]; then
  echo "Starting backend in background with RUST_LOG=${RUST_LOG}"
  nohup "${DIST_DIR}/vid-archi-backend" > "${LOG_DIR}/backend.log" 2>&1 &
  echo $! > "${PID_FILE}"
  echo "Backend PID $(cat "${PID_FILE}")"
else
  exec "${DIST_DIR}/vid-archi-backend"
fi
