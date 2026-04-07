#!/usr/bin/env bash
# Run the elephc test suite inside a Linux ARM64 Docker container.
#
# Usage:
#   ./scripts/test-linux.sh                # run all tests
#   ./scripts/test-linux.sh test_fizz      # run tests matching a pattern
#   ./scripts/test-linux.sh --rebuild      # force rebuild the Docker image
#
set -euo pipefail

IMAGE="elephc-test-linux"
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
    echo "Building Docker image '$IMAGE'..."
    docker build -t "$IMAGE" -f "$PROJECT_DIR/Dockerfile.test-linux" "$PROJECT_DIR"
fi

# Run tests with the project mounted as a volume
if [ ${#TEST_ARGS[@]} -eq 0 ]; then
    echo "Running all tests on Linux..."
    docker run --rm -v "$PROJECT_DIR:/app" -w /app "$IMAGE" cargo test
else
    echo "Running tests matching '${TEST_ARGS[*]}' on Linux..."
    docker run --rm -v "$PROJECT_DIR:/app" -w /app "$IMAGE" cargo test "${TEST_ARGS[@]}"
fi
