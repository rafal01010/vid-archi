#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env"
RUN_CHECKS=0
INSTALL_FRONTEND_DEPS=1
START_SERVICES=1
BOOTSTRAP_DB=0
RUN_DB_MIGRATIONS=0
RESET_DB=0

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
    --bootstrap-db)
      BOOTSTRAP_DB=1
      RUN_DB_MIGRATIONS=1
      shift
      ;;
    --reset-db)
      RESET_DB=1
      BOOTSTRAP_DB=1
      RUN_DB_MIGRATIONS=1
      shift
      ;;
    --migrate-db)
      RUN_DB_MIGRATIONS=1
      shift
      ;;
    --skip-install-frontend-deps)
      INSTALL_FRONTEND_DEPS=0
      shift
      ;;
    --install-frontend-deps)
      INSTALL_FRONTEND_DEPS=1
      shift
      ;;
    --skip-start-services)
      START_SERVICES=0
      shift
      ;;
    --start-services)
      START_SERVICES=1
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--env-file FILE] [--run-checks] [--bootstrap-db] [--reset-db] [--migrate-db] [--skip-install-frontend-deps] [--skip-start-services]"
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
  echo "docker is not installed. Install Docker before deploying STG-Lite."
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is not installed. Install the Rust toolchain before deploying STG-Lite."
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "npm is not installed. Install Node.js before deploying STG-Lite."
  exit 1
fi

POSTGRES_CONTAINER_NAME="${STG_POSTGRES_CONTAINER_NAME:-stg-postgres}"
POSTGRES_DB_VALUE="${STG_POSTGRES_DB:-${POSTGRES_DB:-video_stg}}"
POSTGRES_USER_VALUE="${STG_POSTGRES_USER:-${POSTGRES_USER:-video_app}}"
POSTGRES_PASSWORD_VALUE="${STG_POSTGRES_PASSWORD:-${POSTGRES_PASSWORD:-change_me}}"
POSTGRES_PORT_VALUE="${STG_POSTGRES_PORT:-${POSTGRES_PORT:-5432}}"
POSTGRES_BIND_ADDRESS_VALUE="${STG_POSTGRES_BIND_ADDRESS:-127.0.0.1}"
POSTGRES_VOLUME_NAME="${STG_POSTGRES_VOLUME_NAME:-pgdata}"
MIGRATIONS_DIR="${REPO_ROOT}/backend/migrations"

ensure_postgres_container() {
  if [[ "${RESET_DB}" == "1" ]]; then
    echo "Resetting Postgres container ${POSTGRES_CONTAINER_NAME} and volume ${POSTGRES_VOLUME_NAME}"
    docker rm -f "${POSTGRES_CONTAINER_NAME}" >/dev/null 2>&1 || true
    docker volume rm "${POSTGRES_VOLUME_NAME}" >/dev/null 2>&1 || true
  fi

  if ! docker container inspect "${POSTGRES_CONTAINER_NAME}" >/dev/null 2>&1; then
    if [[ "${BOOTSTRAP_DB}" != "1" ]]; then
      echo "Postgres container ${POSTGRES_CONTAINER_NAME} does not exist."
      echo "Run this command once with --bootstrap-db for first-time database setup."
      exit 1
    fi

    echo "Creating STG Postgres container ${POSTGRES_CONTAINER_NAME}"
    docker run -d \
      --name "${POSTGRES_CONTAINER_NAME}" \
      --restart unless-stopped \
      -e POSTGRES_DB="${POSTGRES_DB_VALUE}" \
      -e POSTGRES_USER="${POSTGRES_USER_VALUE}" \
      -e POSTGRES_PASSWORD="${POSTGRES_PASSWORD_VALUE}" \
      -v "${POSTGRES_VOLUME_NAME}:/var/lib/postgresql/data" \
      -p "${POSTGRES_BIND_ADDRESS_VALUE}:${POSTGRES_PORT_VALUE}:5432" \
      postgres:16 \
      -c listen_addresses='*'
  else
    echo "Starting existing STG Postgres container ${POSTGRES_CONTAINER_NAME}"
    docker start "${POSTGRES_CONTAINER_NAME}" >/dev/null 2>&1 || true
  fi
}

wait_for_postgres() {
  echo "Waiting for Postgres to become ready"
  until docker exec "${POSTGRES_CONTAINER_NAME}" \
    pg_isready -U "${POSTGRES_USER_VALUE}" -d "${POSTGRES_DB_VALUE}" >/dev/null 2>&1; do
    sleep 2
  done
}

apply_pending_migrations() {
  if [[ ! -d "${MIGRATIONS_DIR}" ]]; then
    echo "Missing migrations directory: ${MIGRATIONS_DIR}"
    exit 1
  fi

  shopt -s nullglob
  local migration_files=("${MIGRATIONS_DIR}"/*.up.sql)

  if [[ ${#migration_files[@]} -eq 0 ]]; then
    echo "No migration files found in ${MIGRATIONS_DIR}"
    exit 1
  fi

  docker exec -i "${POSTGRES_CONTAINER_NAME}" \
    psql \
    -v ON_ERROR_STOP=1 \
    -U "${POSTGRES_USER_VALUE}" \
    -d "${POSTGRES_DB_VALUE}" <<'SQL'
CREATE TABLE IF NOT EXISTS schema_migrations (
  filename TEXT PRIMARY KEY,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
SQL

  local migration_history_count
  migration_history_count="$(
    docker exec "${POSTGRES_CONTAINER_NAME}" \
      psql \
      -tA \
      -U "${POSTGRES_USER_VALUE}" \
      -d "${POSTGRES_DB_VALUE}" \
      -c "SELECT COUNT(*) FROM schema_migrations;"
  )"

  if [[ "${migration_history_count}" == "0" ]]; then
    local existing_schema_count
    existing_schema_count="$(
      docker exec "${POSTGRES_CONTAINER_NAME}" \
        psql \
        -tA \
        -U "${POSTGRES_USER_VALUE}" \
        -d "${POSTGRES_DB_VALUE}" \
        -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('videos', 'upload_sessions', 'upload_parts', 'video_renditions', 'processing_jobs');"
    )"

    if [[ "${existing_schema_count}" == "5" ]]; then
      echo "Detected an existing application schema without migration history. Recording current migrations as already applied."

      for migration_file in "${migration_files[@]}"; do
        local migration_basename
        migration_basename="$(basename "${migration_file}")"
        docker exec "${POSTGRES_CONTAINER_NAME}" \
          psql \
          -v ON_ERROR_STOP=1 \
          -U "${POSTGRES_USER_VALUE}" \
          -d "${POSTGRES_DB_VALUE}" \
          -c "INSERT INTO schema_migrations (filename) VALUES ('${migration_basename}') ON CONFLICT (filename) DO NOTHING;" >/dev/null
      done

      return
    fi
  fi

  for migration_file in "${migration_files[@]}"; do
    local migration_basename
    migration_basename="$(basename "${migration_file}")"
    local already_applied
    already_applied="$(
      docker exec "${POSTGRES_CONTAINER_NAME}" \
        psql \
        -tA \
        -U "${POSTGRES_USER_VALUE}" \
        -d "${POSTGRES_DB_VALUE}" \
        -c "SELECT 1 FROM schema_migrations WHERE filename = '${migration_basename}' LIMIT 1;"
    )"

    if [[ "${already_applied}" == "1" ]]; then
      echo "Skipping already-applied migration: ${migration_basename}"
      continue
    fi

    echo "Applying migration: ${migration_basename}"
    docker exec -i "${POSTGRES_CONTAINER_NAME}" \
      psql \
      -v ON_ERROR_STOP=1 \
      -U "${POSTGRES_USER_VALUE}" \
      -d "${POSTGRES_DB_VALUE}" \
      < "${migration_file}"

    docker exec "${POSTGRES_CONTAINER_NAME}" \
      psql \
      -v ON_ERROR_STOP=1 \
      -U "${POSTGRES_USER_VALUE}" \
      -d "${POSTGRES_DB_VALUE}" \
      -c "INSERT INTO schema_migrations (filename) VALUES ('${migration_basename}');" >/dev/null
  done
}

ensure_postgres_container
wait_for_postgres

if [[ "${RUN_DB_MIGRATIONS}" == "1" ]]; then
  apply_pending_migrations
else
  echo "Skipping DB bootstrap/migrations. Use --bootstrap-db for first-time setup or --migrate-db when applying new migrations."
fi

BACKEND_DEPLOY_ARGS=()
FRONTEND_DEPLOY_ARGS=(--env-file "${ENV_FILE}")

if [[ "${RUN_CHECKS}" == "1" ]]; then
  BACKEND_DEPLOY_ARGS+=(--run-tests)
  FRONTEND_DEPLOY_ARGS+=(--run-check)
fi

echo "Packaging backend"
"${REPO_ROOT}/backend/scripts/deploy-backend.sh" "${BACKEND_DEPLOY_ARGS[@]}"

echo "Packaging frontend"
if [[ "${INSTALL_FRONTEND_DEPS}" == "1" ]]; then
  FRONTEND_DEPLOY_ARGS+=(--install)
fi
"${REPO_ROOT}/frontend/scripts/deploy-frontend.sh" "${FRONTEND_DEPLOY_ARGS[@]}"

if [[ "${START_SERVICES}" == "1" ]]; then
  "${SCRIPT_DIR}/start-stg-lite.sh" --env-file "${ENV_FILE}"
fi

echo "STG-Lite deployment assets are ready."
echo "Postgres container: ${POSTGRES_CONTAINER_NAME}"
echo "Postgres published on: ${POSTGRES_BIND_ADDRESS_VALUE}:${POSTGRES_PORT_VALUE}"
echo "Backend dist: ${REPO_ROOT}/backend/dist"
echo "Frontend build artifacts: ${REPO_ROOT}/frontend/.svelte-kit/output"
