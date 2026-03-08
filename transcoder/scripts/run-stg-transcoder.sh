#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
DIST_DIR="${REPO_ROOT}/transcoder/dist"
LOG_DIR="${REPO_ROOT}/logs"
BACKGROUND=0
RENDITION="${TRANSCODER_RENDITION:-}"
INSTANCE_LABEL=""

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
    --rendition)
      RENDITION="$2"
      shift 2
      ;;
    --instance-label)
      INSTANCE_LABEL="$2"
      shift 2
      ;;
    --background)
      BACKGROUND=1
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--dist-dir DIR] [--rendition NAME] [--instance-label LABEL] [--background]"
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

if [[ ! -x "${DIST_DIR}/vid-archi-transcoder" ]]; then
  echo "Missing transcoder binary at ${DIST_DIR}/vid-archi-transcoder. Run ./transcoder/scripts/deploy-transcoder.sh first."
  exit 1
fi

if [[ -z "${RENDITION}" ]]; then
  echo "TRANSCODER_RENDITION is required."
  exit 1
fi

export DATABASE_URL="${STG_DATABASE_URL:-${DATABASE_URL:-}}"
export TRANSCODER_RENDITION="${RENDITION}"

if [[ -z "${DATABASE_URL}" ]]; then
  echo "DATABASE_URL is not set. Provide STG_DATABASE_URL or DATABASE_URL in the env file."
  exit 1
fi

mkdir -p "${LOG_DIR}"

if [[ -z "${INSTANCE_LABEL}" ]]; then
  INSTANCE_LABEL="${RENDITION}"
fi

if [[ "${BACKGROUND}" == "1" ]]; then
  local_log_file="${LOG_DIR}/transcoder-${INSTANCE_LABEL}.log"
  local_pid_file="${LOG_DIR}/transcoder-${INSTANCE_LABEL}.pid"
  echo "Starting transcoder ${INSTANCE_LABEL} in background"
  nohup "${DIST_DIR}/vid-archi-transcoder" > "${local_log_file}" 2>&1 &
  echo $! > "${local_pid_file}"
  echo "Transcoder PID $(cat "${local_pid_file}")"
else
  exec "${DIST_DIR}/vid-archi-transcoder"
fi
