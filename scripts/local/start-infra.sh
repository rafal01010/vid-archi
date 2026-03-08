#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib.sh"

"${SCRIPT_DIR}/prepare-env.sh"
ensure_container_runtime_for_start

cd "${REPO_ROOT}"
docker compose up -d

cat <<'EOF'
Local infrastructure is starting.

Expected local services:
- Postgres: http://127.0.0.1:5432
- MinIO API: http://127.0.0.1:9000
- MinIO Console: http://127.0.0.1:9001

Current scope after checklist step 1:
- Local infrastructure is available.
- Buckets are created by the bootstrap container.
- Backend, worker, and frontend app processes do not exist yet.

Next useful command:
- ./scripts/local/status-infra.sh
EOF
