#!/usr/bin/env bash
set -euo pipefail
if [ "$#" -ne 0 ]; then
    echo 'Usage: ./run_playground.sh' >&2
    exit 2
fi
PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
command -v python3 >/dev/null || { echo 'python3 is required for playground startup checks' >&2; exit 1; }
"$PROJECT_ROOT/build_docker.sh"
mkdir -p "$PROJECT_ROOT/playground/data" "$PROJECT_ROOT/playground/upload" "$PROJECT_ROOT/playground/output"
export HELPYOURSELF_CONFIG="$PROJECT_ROOT/playground/config.toml"
export HELPYOURSELF_DATA_DIR="$PROJECT_ROOT/playground/data"
COMPOSE=(docker compose --project-name helpyourself-playground --file "$PROJECT_ROOT/docker/docker-compose.yaml")
"${COMPOSE[@]}" config --format json | python3 "$PROJECT_ROOT/src/development/playground.py"
exec "${COMPOSE[@]}" up --no-build
