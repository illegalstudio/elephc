#!/usr/bin/env bash
# Run the elephc test suite inside a Linux x86_64 Docker container.
#
# Usage:
#   ./scripts/test-linux-x86_64.sh                # run all tests
#   ./scripts/test-linux-x86_64.sh test_fizz      # run tests matching a pattern
#   ./scripts/test-linux-x86_64.sh --rebuild      # force rebuild the Docker image
#
# The Cargo target volume is temporary per run and removed during cleanup.
# Override with ELEPHC_DOCKER_TARGET_VOLUME; set ELEPHC_KEEP_DOCKER_TARGET_VOLUME=1
# to keep it for debugging.
# CPU and memory are capped by default. A disk guard stops the test if its
# target volume grows too large or Docker's filesystem runs low on free space.
# Override the defaults with ELEPHC_DOCKER_CPUS, ELEPHC_DOCKER_MEMORY,
# ELEPHC_DOCKER_MAX_TARGET_GIB, ELEPHC_DOCKER_MIN_FREE_GIB, and
# ELEPHC_DOCKER_DISK_CHECK_SECONDS.
#
set -euo pipefail

IMAGE="elephc-test-linux-x86_64"
PLATFORM="linux/amd64"
CONTAINER_NAME="elephc-test-linux-x86_64-$$"
TEST_THREADS="${ELEPHC_TEST_THREADS:-1}"
DOCKER_CPUS="${ELEPHC_DOCKER_CPUS:-1}"
DOCKER_MEMORY="${ELEPHC_DOCKER_MEMORY:-8g}"
MAX_TARGET_GIB="${ELEPHC_DOCKER_MAX_TARGET_GIB:-24}"
MIN_FREE_GIB="${ELEPHC_DOCKER_MIN_FREE_GIB:-20}"
DISK_CHECK_SECONDS="${ELEPHC_DOCKER_DISK_CHECK_SECONDS:-30}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/docker_test_guard.sh
. "$SCRIPT_DIR/docker_test_guard.sh"
for value in "$MAX_TARGET_GIB" "$MIN_FREE_GIB" "$DISK_CHECK_SECONDS"; do
    if [[ ! "$value" =~ ^[1-9][0-9]*$ ]]; then
        echo "Docker disk limits and check interval must be positive integers." >&2
        exit 2
    fi
done
MAX_TARGET_KIB=$((MAX_TARGET_GIB * 1048576))
MIN_FREE_KIB=$((MIN_FREE_GIB * 1048576))
if command -v sha256sum >/dev/null 2>&1; then
    WORKTREE_SHA="$(printf '%s' "$PROJECT_DIR" | sha256sum | awk '{print substr($1, 1, 16)}')"
else
    WORKTREE_SHA="$(printf '%s' "$PROJECT_DIR" | shasum -a 256 | awk '{print substr($1, 1, 16)}')"
fi
TARGET_VOLUME="${ELEPHC_DOCKER_TARGET_VOLUME:-elephc-target-linux-x86_64-$WORKTREE_SHA-$$}"
KEEP_TARGET_VOLUME="${ELEPHC_KEEP_DOCKER_TARGET_VOLUME:-0}"
DOCKERFILE="$PROJECT_DIR/Dockerfile.test-linux-x86_64"
DOCKER_ROOT="$(docker info -f '{{.DockerRootDir}}')"
AVAILABLE_KIB="$(df -Pk "$DOCKER_ROOT" | awk 'NR == 2 {print $4}')"
if (( AVAILABLE_KIB < MIN_FREE_KIB )); then
    echo "Docker has less than $MIN_FREE_GIB GiB free; refusing to start the test." >&2
    exit 1
fi
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

GUARD_DIR="$(mktemp -d)"
GUARD_PID=""
# shellcheck disable=SC2329 # Invoked by the EXIT trap.
cleanup() {
    if [ -n "$GUARD_PID" ]; then
        kill "$GUARD_PID" >/dev/null 2>&1 || true
        wait "$GUARD_PID" 2>/dev/null || true
    fi
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    if [ "$KEEP_TARGET_VOLUME" != "1" ]; then
        docker volume rm "$TARGET_VOLUME" >/dev/null 2>&1 || true
    fi
    rm -f "$GUARD_DIR/limit_hit"
    rmdir "$GUARD_DIR"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Run tests with the project mounted as a volume. Build the bridge staticlib
# crates first so libelephc_tls.a / libelephc_pdo.a / libelephc_crypto.a /
# libelephc_bcmath.a / libelephc_iconv.a / libelephc_phar.a / libelephc_tz.a /
# libelephc_image.a / libelephc_web.a / libelephc_pcntl.a / libelephc_xml.a /
# libelephc_magician.a / libelephc_curl.a exist in the target dir —
# `cargo test` alone never emits the staticlib crate-type.
#
# Then materialize the managed native libxml2 package (the xml bridge's parser
# is libxml2 itself; see tests/codegen/support/xml_native.rs) with the compiler
# just built, into a cache that lives on the target volume
# (ELEPHC_NATIVE_CACHE) so a kept volume reuses the from-source build. The two
# exported variables make the run COVER xml rather than skip it:
# ELEPHC_XML_LIBXML2_LIB_DIR compiles the crate's libxml2-calling unit tests in
# (crates/elephc-xml/build.rs), and ELEPHC_TEST_REQUIRE_XML_NATIVE turns a
# missing artifact into a loud failure in tests/codegen/xml instead of a skip.
# shellcheck disable=SC2016 # Expanded by the container shell.
DOCKER_TEST_COMMAND='cargo build -p elephc-tls -p elephc-pdo -p elephc-crypto -p elephc-bcmath -p elephc-iconv -p elephc-phar -p elephc-tz -p elephc-image -p elephc-web -p elephc-pcntl -p elephc-xml -p elephc-magician -p elephc-instr -p elephc-probe -p elephc-curl \
    && cargo build --bin elephc \
    && "$CARGO_TARGET_DIR/debug/elephc" native install --locked --target linux-x86_64 --manifest-path examples/xml/elephc.toml \
    && ELEPHC_XML_LIBXML2_LIB_DIR="$(sh scripts/ci/libxml2_lib_dir.sh linux-x86_64)" \
       ELEPHC_TEST_REQUIRE_XML_NATIVE=1 \
       cargo test "$@"'
if [ "$TEST_ARG_COUNT" -eq 0 ]; then
    echo "Running all tests on Linux x86_64 using temporary target volume '$TARGET_VOLUME'..."
else
    echo "Running tests matching '${TEST_ARGS[*]}' on Linux x86_64 using temporary target volume '$TARGET_VOLUME'..."
fi
echo "Limits: $DOCKER_CPUS CPU, $DOCKER_MEMORY memory, $MAX_TARGET_GIB GiB target, $MIN_FREE_GIB GiB minimum free."
docker_test_guard "$CONTAINER_NAME" "$GUARD_DIR/limit_hit" "$MAX_TARGET_KIB" "$MIN_FREE_KIB" "$DISK_CHECK_SECONDS" &
GUARD_PID=$!
RUN_STATUS=0
docker run \
    --platform "$PLATFORM" \
    --name "$CONTAINER_NAME" \
    --init \
    --rm \
    --cpus "$DOCKER_CPUS" \
    --memory "$DOCKER_MEMORY" \
    --memory-swap "$DOCKER_MEMORY" \
    -e "RUST_TEST_THREADS=$TEST_THREADS" \
    -e "CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1}" \
    -e "CARGO_INCREMENTAL=0" \
    -e "CARGO_TARGET_DIR=/cargo-target" \
    -e "ELEPHC_NATIVE_CACHE=/cargo-target/elephc-native" \
    -v "$PROJECT_DIR:/app" \
    -v "$TARGET_VOLUME:/cargo-target" \
    -w /app \
    "$IMAGE" \
    sh -c "$DOCKER_TEST_COMMAND" sh "${TEST_ARGS[@]}" || RUN_STATUS=$?
if [ -e "$GUARD_DIR/limit_hit" ]; then
    exit 1
fi
exit "$RUN_STATUS"
