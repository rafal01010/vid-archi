#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib.sh"

"${SCRIPT_DIR}/prepare-env.sh"
ensure_container_runtime_is_ready
source "${REPO_ROOT}/.env"

MIGRATIONS_DIR="${REPO_ROOT}/backend/migrations"

if [[ ! -d "${MIGRATIONS_DIR}" ]]; then
  echo "Missing migrations directory: ${MIGRATIONS_DIR}"
  exit 1
fi

cd "${REPO_ROOT}"

if ! docker compose exec -T postgres pg_isready -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" >/dev/null 2>&1; then
  echo "Postgres container is not ready. Start local infra first:"
  echo "./scripts/local/start-infra.sh"
  exit 1
fi

shopt -s nullglob
migration_files=("${MIGRATIONS_DIR}"/*.up.sql)

if [[ ${#migration_files[@]} -eq 0 ]]; then
  echo "No migration files found in ${MIGRATIONS_DIR}"
  exit 0
fi

for migration_file in "${migration_files[@]}"; do
  echo "Applying migration: $(basename "${migration_file}")"
  docker compose exec -T postgres \
    psql \
    -v ON_ERROR_STOP=1 \
    -U "${POSTGRES_USER}" \
    -d "${POSTGRES_DB}" \
    < "${migration_file}"
done

echo "Database migrations applied successfully."
