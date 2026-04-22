#!/usr/bin/env bash
# Run the elephc test suite inside a Linux x86_64 Docker container.
#
# Usage:
#   ./scripts/test-linux-x86_64.sh                # run all tests
#   ./scripts/test-linux-x86_64.sh test_fizz      # run tests matching a pattern
#   ./scripts/test-linux-x86_64.sh --rebuild      # force rebuild the Docker image
#
set -euo pipefail

IMAGE="elephc-test-linux-x86_64"
PLATFORM="linux/amd64"
TARGET_VOLUME="elephc-target-linux-x86_64"
CONTAINER_NAME="elephc-test-linux-x86_64-$$"
TEST_THREADS="${ELEPHC_TEST_THREADS:-1}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

REBUILD=false
TEST_ARGS=()

for arg in "$@"; do
    case "$arg" in
        --rebuild) REBUILD=true ;;
        *)         TEST_ARGS+=("$arg") ;;
    esac
done

# Build the image if it doesn't exist or --rebuild was passed
if $REBUILD || ! docker image inspect "$IMAGE" &>/dev/null; then
    echo "Building Docker image '$IMAGE' for $PLATFORM..."
    docker build --platform "$PLATFORM" -t "$IMAGE" -f "$PROJECT_DIR/Dockerfile.test-linux-x86_64" "$PROJECT_DIR"
fi

cleanup() {
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}

trap cleanup EXIT INT TERM

# Run tests with the project mounted as a volume
if [ ${#TEST_ARGS[@]} -eq 0 ]; then
    echo "Running all tests on Linux x86_64 with RUST_TEST_THREADS=$TEST_THREADS..."
    docker run \
        --platform "$PLATFORM" \
        --name "$CONTAINER_NAME" \
        --init \
        --rm \
        -e "RUST_TEST_THREADS=$TEST_THREADS" \
        -e "CARGO_TARGET_DIR=/cargo-target" \
        -v "$PROJECT_DIR:/app" \
        -v "$TARGET_VOLUME:/cargo-target" \
        -w /app \
        "$IMAGE" \
        cargo test
else
    echo "Running tests matching '${TEST_ARGS[*]}' on Linux x86_64 with RUST_TEST_THREADS=$TEST_THREADS..."
    docker run \
        --platform "$PLATFORM" \
        --name "$CONTAINER_NAME" \
        --init \
        --rm \
        -e "RUST_TEST_THREADS=$TEST_THREADS" \
        -e "CARGO_TARGET_DIR=/cargo-target" \
        -v "$PROJECT_DIR:/app" \
        -v "$TARGET_VOLUME:/cargo-target" \
        -w /app \
        "$IMAGE" \
        cargo test "${TEST_ARGS[@]}"
fi
