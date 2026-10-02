#!/usr/bin/env bash
set -euo pipefail
if [ "$#" -gt 1 ]; then
    echo 'Usage: ./build_docker.sh [tag]' >&2
    exit 1
fi
PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
IMAGE_TAG="${1:-local}"
if [[ ! "$IMAGE_TAG" =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.-]{0,127}$ ]]; then
    echo 'Invalid Docker tag' >&2
    exit 2
fi
python3 "$PROJECT_ROOT/tests/development/playground.py"
python3 -m unittest discover -s "$PROJECT_ROOT/tests/development" -p 'ocr_key.py'
python3 "$PROJECT_ROOT/tests/run_playground.py"
python3 "$PROJECT_ROOT/tests/build_ios.py"
cd "$PROJECT_ROOT/docker"
docker build --file backend_api.Dockerfile --tag "helpyourself-backend-api:$IMAGE_TAG" ..
exec docker build --file backend_ocr.Dockerfile --tag "helpyourself-backend-ocr:$IMAGE_TAG" ..
