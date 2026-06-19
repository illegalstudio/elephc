#!/usr/bin/env bash
# Run the elephc test suite inside a Linux ARM64 Docker container.
#
# Usage:
#   ./scripts/test-linux-arm64.sh                # run all tests
#   ./scripts/test-linux-arm64.sh test_fizz      # run tests matching a pattern
#   ./scripts/test-linux-arm64.sh --rebuild      # force rebuild the Docker image
#
# The Cargo target volume is temporary per run and removed during cleanup.
# Override with ELEPHC_DOCKER_TARGET_VOLUME; set ELEPHC_KEEP_DOCKER_TARGET_VOLUME=1
# to keep it for debugging.
#
set -euo pipefail

IMAGE="elephc-test-linux-arm64"
PLATFORM="linux/arm64"
CONTAINER_NAME="elephc-test-linux-arm64-$$"
TEST_THREADS="${ELEPHC_TEST_THREADS:-1}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
if command -v sha256sum >/dev/null 2>&1; then
    WORKTREE_SHA="$(printf '%s' "$PROJECT_DIR" | sha256sum | awk '{print substr($1, 1, 16)}')"
else
    WORKTREE_SHA="$(printf '%s' "$PROJECT_DIR" | shasum -a 256 | awk '{print substr($1, 1, 16)}')"
fi
TARGET_VOLUME="${ELEPHC_DOCKER_TARGET_VOLUME:-elephc-target-linux-arm64-$WORKTREE_SHA-$$}"
KEEP_TARGET_VOLUME="${ELEPHC_KEEP_DOCKER_TARGET_VOLUME:-0}"
DOCKERFILE="$PROJECT_DIR/Dockerfile.test-linux-arm64"
if command -v sha256sum >/dev/null 2>&1; then
    DOCKERFILE_SHA="$(sha256sum "$DOCKERFILE" | awk '{print $1}')"
else
    DOCKERFILE_SHA="$(shasum -a 256 "$DOCKERFILE" | awk '{print $1}')"
fi

REBUILD=false
TEST_ARGS=()
TEST_ARG_COUNT=0

for arg in "$@"; do
    case "$arg" in
        --rebuild) REBUILD=true ;;
        *)
            TEST_ARGS+=("$arg")
            TEST_ARG_COUNT=$((TEST_ARG_COUNT + 1))
            ;;
    esac
done

# Build the image if it doesn't exist, --rebuild was passed, or the Dockerfile changed.
IMAGE_DOCKERFILE_SHA="$(docker image inspect -f '{{ index .Config.Labels "elephc.dockerfile-sha" }}' "$IMAGE" 2>/dev/null || true)"
if $REBUILD || [ "$IMAGE_DOCKERFILE_SHA" != "$DOCKERFILE_SHA" ]; then
    echo "Building Docker image '$IMAGE' for $PLATFORM..."
    docker build \
        --platform "$PLATFORM" \
        --label "elephc.dockerfile-sha=$DOCKERFILE_SHA" \
        -t "$IMAGE" \
        -f "$DOCKERFILE" \
        "$PROJECT_DIR"
fi

cleanup() {
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    if [ "$KEEP_TARGET_VOLUME" != "1" ]; then
        docker volume rm "$TARGET_VOLUME" >/dev/null 2>&1 || true
    fi
}

trap cleanup EXIT INT TERM

# Run tests with the project mounted as a volume. Build the bridge staticlib
# crates first so libelephc_tls.a / libelephc_pdo.a / libelephc_crypto.a /
# libelephc_phar.a / libelephc_tz.a exist in the target dir — `cargo test` alone
# never emits the staticlib crate-type.
if [ "$TEST_ARG_COUNT" -eq 0 ]; then
    echo "Running all tests on Linux ARM64 with RUST_TEST_THREADS=$TEST_THREADS using temporary target volume '$TARGET_VOLUME'..."
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
        sh -c 'cargo build -p elephc-tls -p elephc-pdo -p elephc-crypto -p elephc-phar -p elephc-tz && cargo test'
else
    echo "Running tests matching '${TEST_ARGS[*]}' on Linux ARM64 with RUST_TEST_THREADS=$TEST_THREADS using temporary target volume '$TARGET_VOLUME'..."
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
        sh -c 'cargo build -p elephc-tls -p elephc-pdo -p elephc-crypto -p elephc-phar -p elephc-tz && cargo test "$@"' sh "${TEST_ARGS[@]}"
fi
