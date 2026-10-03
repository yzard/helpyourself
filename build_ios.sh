#!/usr/bin/env bash
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
CORE_ONLY=false
TEST_DESTINATION=''
while [ "$#" -gt 0 ]; do
    case "$1" in
        --core-only) CORE_ONLY=true; shift ;;
        --test-destination)
            if [ "$#" -lt 2 ] || [ -z "$2" ]; then echo '--test-destination requires an Xcode destination' >&2; exit 2; fi
            TEST_DESTINATION="$2"; shift 2 ;;
        --help) echo 'Usage: ./build_ios.sh [--core-only | --test-destination DESTINATION]'; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
if $CORE_ONLY && [ -n "$TEST_DESTINATION" ]; then
    echo '--core-only cannot run Apple SDK tests' >&2; exit 2
fi
cd "$PROJECT_ROOT"
if [ "$(uname -s)" != Darwin ]; then
    if ! $CORE_ONLY; then echo 'Full iOS builds require macOS/Xcode. --core-only checks portable code without claiming an iOS build.' >&2; exit 1; fi
    exec docker run --rm --user "$(id -u):$(id -g)" -v "$PROJECT_ROOT:/workspace" -w /workspace swift:6.2 \
        bash -c 'swiftc -frontend -parse src/ios/*.swift src/ios/Core/*.swift && swift test --scratch-path build/ios/core'
fi
swift test --scratch-path build/ios/core
if $CORE_ONLY; then exit 0; fi
command -v xcodegen >/dev/null || { echo 'Install XcodeGen before building iOS' >&2; exit 1; }
command -v xcodebuild >/dev/null || { echo 'Select a full Xcode installation before building iOS' >&2; exit 1; }
mkdir -p build/ios dist/ios
xcodegen generate --spec src/ios/project.yml --project build/ios
XCODE=(xcodebuild -project build/ios/Helpyourself.xcodeproj -scheme Helpyourself -derivedDataPath build/ios/DerivedData)
"${XCODE[@]}" -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' CODE_SIGN_IDENTITY=- build-for-testing
if [ -n "$TEST_DESTINATION" ]; then
    # Native tests use synthetic data and a loopback-only API fixture.
    python3 tests/ios/simulator_fixture.py > build/ios/fixture.log 2>&1 &
    FIXTURE_PID=$!
    trap 'kill "$FIXTURE_PID" 2>/dev/null || true' EXIT
    python3 - "$FIXTURE_PID" <<'PY'
import os, sys, time, urllib.request
for attempt in range(50):
    os.kill(int(sys.argv[1]), 0)
    try:
        urllib.request.urlopen('http://127.0.0.1:18765/api/v1/test/ping', timeout=1).close()
        break
    except OSError:
        time.sleep(0.1)
else:
    raise SystemExit('Simulator API fixture did not start')
PY
    "${XCODE[@]}" -sdk iphonesimulator -destination "$TEST_DESTINATION" -parallel-testing-enabled NO CODE_SIGN_IDENTITY=- test-without-building
fi
"${XCODE[@]}" -sdk iphoneos -destination 'generic/platform=iOS' -configuration Release CODE_SIGNING_ALLOWED=NO build
rm -rf "$PROJECT_ROOT/dist/ios/Helpyourself.app"
ditto build/ios/DerivedData/Build/Products/Release-iphoneos/Helpyourself.app dist/ios/Helpyourself.app
echo 'Unsigned device app: dist/ios/Helpyourself.app. Signing and provisioning are required to install it.'
