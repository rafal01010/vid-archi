#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
COMPOSE_FILE="${REPO_ROOT}/chunker/docker-compose.yml"
SERVICES=("chunker")
SCALE_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      ENV_FILE="$2"
      shift 2
      ;;
    --service)
      SERVICES=("$2")
      shift 2
      ;;
    --scale)
      SCALE_ARGS+=("--scale" "$2")
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--service NAME] [--scale chunker=count]"
      exit 1
      ;;
  esac
done

docker compose \
  --env-file "${ENV_FILE}" \
  -f "${COMPOSE_FILE}" \
  up -d "${SCALE_ARGS[@]}" "${SERVICES[@]}"
