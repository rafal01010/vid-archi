#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib.sh"

if [[ $# -ne 1 ]]; then
  echo "Usage: ./scripts/local/db-restore.sh <backup-file.sql>"
  exit 1
fi

BACKUP_FILE="$1"

if [[ ! -f "${BACKUP_FILE}" ]]; then
  echo "Backup file not found: ${BACKUP_FILE}"
  exit 1
fi

"${SCRIPT_DIR}/prepare-env.sh"
ensure_container_runtime_is_ready
source "${REPO_ROOT}/.env"

cd "${REPO_ROOT}"

if ! docker compose exec -T postgres pg_isready -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" >/dev/null 2>&1; then
  echo "Postgres container is not ready. Start local infra first:"
  echo "./scripts/local/start-infra.sh"
  exit 1
fi

echo "Restoring ${BACKUP_FILE} into database ${POSTGRES_DB}"
docker compose exec -T postgres \
  psql \
  -v ON_ERROR_STOP=1 \
  -U "${POSTGRES_USER}" \
  -d "${POSTGRES_DB}" \
  < "${BACKUP_FILE}"

echo "Database restore completed."
