#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
HOST="0.0.0.0"
PORT=""
LOG_DIR="${REPO_ROOT}/logs"
BACKGROUND=0

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

echo "Starting built frontend preview server on http://${HOST}:${PORT}"
echo "Using PUBLIC_API_BASE_URL=${PUBLIC_API_BASE_URL:-<same-origin-relative-with-reverse-proxy>}"

if [[ "${BACKGROUND}" == "1" ]]; then
  mkdir -p "${LOG_DIR}"
  nohup npm run preview -- --host "${HOST}" --port "${PORT}" > "${LOG_DIR}/frontend.log" 2>&1 &
  echo $! > "${LOG_DIR}/frontend.pid"
  echo "Frontend PID $(cat "${LOG_DIR}/frontend.pid")"
else
  exec npm run preview -- --host "${HOST}" --port "${PORT}"
fi
