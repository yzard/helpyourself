#!/usr/bin/env bash
set -euo pipefail
if [ "$#" -gt 1 ]; then
    echo 'Usage: ./build_docker.sh [local-image-tag]' >&2
    exit 1
fi
PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
IMAGE_TAG="${1:-helpyourself:local}"
python3 "$PROJECT_ROOT/tests/development/playground.py"
python3 "$PROJECT_ROOT/tests/run_playground.py"
python3 "$PROJECT_ROOT/tests/build_ios.py"
cd "$PROJECT_ROOT/docker"
exec docker build --file Dockerfile --tag "$IMAGE_TAG" ..

