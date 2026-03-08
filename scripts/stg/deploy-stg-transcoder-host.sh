#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
RUN_CHECKS=0
START_SERVICES=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    --run-checks)
      RUN_CHECKS=1
      shift
      ;;
    --skip-start-services)
      START_SERVICES=0
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--run-checks] [--skip-start-services]"
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

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is not installed. Install Docker before deploying the transcoder host."
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is not installed. Install the Rust toolchain before deploying the transcoder host."
  exit 1
fi

TRANSCODER_DEPLOY_ARGS=()

if [[ "${RUN_CHECKS}" == "1" ]]; then
  TRANSCODER_DEPLOY_ARGS+=(--run-tests)
fi

"${REPO_ROOT}/transcoder/scripts/deploy-transcoder.sh" "${TRANSCODER_DEPLOY_ARGS[@]}"

docker build -t "${TRANSCODER_IMAGE:-vid-archi-transcoder:stg}" -f "${REPO_ROOT}/transcoder/Dockerfile" "${REPO_ROOT}"

if [[ "${START_SERVICES}" == "1" ]]; then
  "${SCRIPT_DIR}/start-stg-transcoder-host.sh" --env-file "${ENV_FILE}"
fi
