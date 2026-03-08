#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib.sh"

"${SCRIPT_DIR}/prepare-env.sh"
ensure_container_runtime_is_ready
source "${REPO_ROOT}/.env"

BACKUP_DIR="${REPO_ROOT}/backups/postgres"
TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
OUTPUT_FILE="${1:-${BACKUP_DIR}/${POSTGRES_DB}-${TIMESTAMP}.sql}"

mkdir -p "${BACKUP_DIR}"

cd "${REPO_ROOT}"

if ! docker compose exec -T postgres pg_isready -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" >/dev/null 2>&1; then
  echo "Postgres container is not ready. Start local infra first:"
  echo "./scripts/local/start-infra.sh"
  exit 1
fi

docker compose exec -T postgres \
  pg_dump \
  --clean \
  --if-exists \
  --no-owner \
  --no-privileges \
  -U "${POSTGRES_USER}" \
  -d "${POSTGRES_DB}" \
  > "${OUTPUT_FILE}"

echo "Database backup written to ${OUTPUT_FILE}"
