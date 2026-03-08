#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
RUN_CHECK=0
INSTALL_DEPS=0
START_PREVIEW=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    --run-check)
      RUN_CHECK=1
      shift
      ;;
    --install)
      INSTALL_DEPS=1
      shift
      ;;
    --start-preview)
      START_PREVIEW=1
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--run-check] [--install] [--start-preview]"
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
  echo "npm is not installed. Install Node.js before deploying the frontend."
  exit 1
fi

if [[ ! -d node_modules ]]; then
  if [[ "${INSTALL_DEPS}" == "1" ]]; then
    echo "Installing frontend dependencies"
    npm install
  else
    echo "Missing frontend/node_modules. Run 'npm --prefix frontend install' or rerun this script with --install."
    exit 1
  fi
fi

if [[ "${APP_ENV:-local}" == "local" ]]; then
  export PUBLIC_API_BASE_URL="${PUBLIC_API_BASE_URL:-http://127.0.0.1:8080}"
elif [[ -n "${STG_ALB_DNS:-}" ]]; then
  export PUBLIC_API_BASE_URL="${PUBLIC_API_BASE_URL:-http://${STG_ALB_DNS}}"
else
  export PUBLIC_API_BASE_URL="${PUBLIC_API_BASE_URL:-}"
fi

if [[ "${RUN_CHECK}" == "1" ]]; then
  echo "Running frontend validation"
  npm run check
fi

echo "Building frontend"
npm run build

echo "Frontend build completed"
echo "PUBLIC_API_BASE_URL=${PUBLIC_API_BASE_URL:-<same-origin-relative-with-reverse-proxy>}"

if [[ "${START_PREVIEW}" == "1" ]]; then
  "${SCRIPT_DIR}/run-stg-frontend.sh" --env-file "${ENV_FILE}"
fi
