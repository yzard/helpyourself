#!/usr/bin/env bash
set -euo pipefail
if [ "$#" -ne 0 ]; then
    echo 'Usage: ./run_playground.sh' >&2
    exit 2
fi
PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
command -v python3 >/dev/null || { echo 'python3 is required for playground startup checks' >&2; exit 1; }
"$PROJECT_ROOT/build_docker.sh"
export HELPYOURSELF_API_DATA_DIR="$PROJECT_ROOT/playground/backend_api"
export HELPYOURSELF_OCR_DATA_DIR="$PROJECT_ROOT/playground/backend_ocr"
export PUID="$(id -u)" PGID="$(id -g)"
COMPOSE=(docker compose --project-name helpyourself-playground --file "$PROJECT_ROOT/docker/docker-compose.yaml")
if [ -e "$PROJECT_ROOT/playground/data" ] || [ -e "$PROJECT_ROOT/playground/config.toml" ] || [ -e "$PROJECT_ROOT/playground/backend_ocr.toml" ] || [ -e "$PROJECT_ROOT/playground/secrets/ocr-key" ]; then
    "${COMPOSE[@]}" stop
fi
python3 "$PROJECT_ROOT/src/development/data_roots.py" --project "$PROJECT_ROOT"
mkdir -p "$PROJECT_ROOT/playground/upload" "$PROJECT_ROOT/playground/output"
"${COMPOSE[@]}" config --format json | python3 "$PROJECT_ROOT/src/development/playground.py"
exec "${COMPOSE[@]}" up --no-build
