#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
HOST="0.0.0.0"
PORT=""
LOG_DIR="${REPO_ROOT}/logs"
BACKGROUND=0
PID_FILE="${LOG_DIR}/frontend.pid"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    --host)
      HOST="$2"
      shift 2
      ;;
    --port)
      PORT="$2"
      shift 2
      ;;
    --background)
      BACKGROUND=1
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--host HOST] [--port PORT] [--background]"
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

cd "${REPO_ROOT}/frontend"

if ! command -v npm >/dev/null 2>&1; then
  echo "npm is not installed. Install Node.js before starting the frontend."
  exit 1
fi

PORT="${PORT:-${FRONTEND_PORT:-4173}}"
if [[ "${APP_ENV:-stg}" == "local" ]]; then
  export PUBLIC_API_BASE_URL="${PUBLIC_API_BASE_URL:-http://127.0.0.1:8080}"
elif [[ -n "${STG_ALB_DNS:-}" ]]; then
  export PUBLIC_API_BASE_URL="${PUBLIC_API_BASE_URL:-http://${STG_ALB_DNS}}"
fi

if [[ -n "${STG_ALB_DNS:-}" ]]; then
  export __VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS="${STG_ALB_DNS}"
fi

stop_existing_frontend() {
  if [[ ! -f "${PID_FILE}" ]]; then
    return
  fi

  local existing_pid
  existing_pid="$(cat "${PID_FILE}")"

  if [[ -n "${existing_pid}" ]] && kill -0 "${existing_pid}" >/dev/null 2>&1; then
    echo "Stopping existing frontend process ${existing_pid}"
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

stop_known_frontend_processes() {
  pkill -u "$(id -u)" -f "npm run preview" >/dev/null 2>&1 || true
  pkill -u "$(id -u)" -f "vite preview" >/dev/null 2>&1 || true
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
  if command -v sudo >/dev/null 2>&1; then
    sudo kill "${listener_pid}" >/dev/null 2>&1 || true
  else
    kill "${listener_pid}" >/dev/null 2>&1 || true
  fi
  sleep 1

  listener_pid="$(find_listening_pid "${port}")"
  if [[ -n "${listener_pid}" ]]; then
    if command -v sudo >/dev/null 2>&1 && command -v fuser >/dev/null 2>&1; then
      echo "Reclaiming ${label} port ${port} with fuser"
      sudo fuser -k "${port}/tcp" >/dev/null 2>&1 || true
    elif command -v sudo >/dev/null 2>&1; then
      echo "Force stopping ${label} port listener ${listener_pid} on ${port}"
      sudo kill -9 "${listener_pid}" >/dev/null 2>&1 || true
    else
      echo "Force stopping ${label} port listener ${listener_pid} on ${port}"
      kill -9 "${listener_pid}" >/dev/null 2>&1 || true
    fi
    sleep 1
  fi
}

wait_for_startup() {
  local pid="$1"
  local port="$2"
  local label="$3"
  local startup_deadline=15

  for (( attempt=1; attempt<=startup_deadline; attempt++ )); do
    if kill -0 "${pid}" >/dev/null 2>&1 && [[ -n "$(find_listening_pid "${port}")" ]]; then
      return 0
    fi

    sleep 1
  done

  echo "${label} failed to start on port ${port}. Recent log output:"
  tail -n 50 "${LOG_DIR}/frontend.log" 2>/dev/null || true
  rm -f "${PID_FILE}"
  return 1
}

echo "Starting built frontend preview server on http://${HOST}:${PORT}"
echo "Using PUBLIC_API_BASE_URL=${PUBLIC_API_BASE_URL:-<same-origin-relative-with-reverse-proxy>}"

mkdir -p "${LOG_DIR}"
echo "" >> "${LOG_DIR}/frontend.log"
echo "=== $(date -Is) frontend start attempt ===" >> "${LOG_DIR}/frontend.log"
stop_existing_frontend
stop_known_frontend_processes
stop_port_listener "${PORT}" "frontend"

if [[ -n "$(find_listening_pid "${PORT}")" ]]; then
  echo "Frontend port ${PORT} is already in use."
  echo "Stop the conflicting process before starting the STG frontend preview server."
  exit 1
fi

if [[ "${BACKGROUND}" == "1" ]]; then
  nohup npm run preview -- --host "${HOST}" --port "${PORT}" --strictPort > "${LOG_DIR}/frontend.log" 2>&1 &
  echo $! > "${PID_FILE}"
  echo "Frontend PID $(cat "${PID_FILE}")"
  wait_for_startup "$(cat "${PID_FILE}")" "${PORT}" "Frontend preview server"
else
  exec npm run preview -- --host "${HOST}" --port "${PORT}" --strictPort
fi
