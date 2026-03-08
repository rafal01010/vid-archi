#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
ENV_EXAMPLE_FILE="${REPO_ROOT}/.env.example"

if [[ ! -f "${ENV_EXAMPLE_FILE}" ]]; then
  echo "Missing ${ENV_EXAMPLE_FILE}"
  exit 1
fi

if [[ -f "${ENV_FILE}" ]]; then
  echo ".env already exists at ${ENV_FILE}"
  exit 0
fi

cp "${ENV_EXAMPLE_FILE}" "${ENV_FILE}"
echo "Created ${ENV_FILE} from .env.example"
