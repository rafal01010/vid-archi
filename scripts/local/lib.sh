#!/usr/bin/env bash

set -euo pipefail

LOCAL_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${LOCAL_SCRIPT_DIR}/../.." && pwd)"

docker_cli_exists() {
  command -v docker >/dev/null 2>&1
}

docker_daemon_is_ready() {
  docker info >/dev/null 2>&1
}

colima_exists() {
  command -v colima >/dev/null 2>&1
}

print_runtime_help() {
  cat <<'EOF'
Container runtime is not available.

On macOS you have two common options:
- Docker Desktop: open the app and wait until Docker reports it is running.
- Colima: run `colima start`

Then retry:
- ./scripts/local/start-infra.sh
EOF
}

ensure_docker_cli() {
  if docker_cli_exists; then
    return 0
  fi

  echo "Docker CLI is required but was not found in PATH."
  echo "Install Docker Desktop or another Docker-compatible CLI first."
  exit 1
}

ensure_container_runtime_for_start() {
  ensure_docker_cli

  if docker_daemon_is_ready; then
    return 0
  fi

  if [[ "$(uname -s)" == "Darwin" ]] && colima_exists; then
    echo "Docker daemon is not running. Starting Colima for local containers..."
    colima start
  fi

  if docker_daemon_is_ready; then
    return 0
  fi

  print_runtime_help
  exit 1
}

ensure_container_runtime_is_ready() {
  ensure_docker_cli

  if docker_daemon_is_ready; then
    return 0
  fi

  print_runtime_help
  exit 1
}
