#!/usr/bin/env bash
# Regression controls for Docker-daemon disk preflight, without a real container or host df.
set -euo pipefail

TEST_SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=../docker_test_guard.sh
. "$TEST_SCRIPT_DIR/../docker_test_guard.sh"

TEST_AVAILABLE_KIB=41943040
TEST_DOCKER_STATUS=0

# The probe must query Docker's filesystem and remove only its own ephemeral container.
docker() {
    [[ "$#" == 9 && "$1" == run && "$2" == --rm && "$3" == --platform &&
        "$4" == linux/arm64 && "$5" == --entrypoint && "$6" == sh &&
        "$7" == test-image && "$8" == -c ]] || return 91
    [[ "$9" == *'df -Pk /'* ]] || return 92
    [[ "$TEST_DOCKER_STATUS" == 0 ]] || return "$TEST_DOCKER_STATUS"
    printf '%s\n' "$TEST_AVAILABLE_KIB"
}

[[ "$(docker_test_available_kib test-image linux/arm64)" == 41943040 ]]
TEST_AVAILABLE_KIB=0
[[ "$(docker_test_available_kib test-image linux/arm64)" == 0 ]]
TEST_AVAILABLE_KIB=unavailable
if docker_test_available_kib test-image linux/arm64 >/dev/null 2>&1; then
    echo "Malformed disk output unexpectedly passed." >&2
    exit 1
fi
TEST_AVAILABLE_KIB=41943040
TEST_DOCKER_STATUS=1
if docker_test_available_kib test-image linux/arm64 >/dev/null 2>&1; then
    echo "A failed Docker probe unexpectedly passed." >&2
    exit 1
fi
echo "Docker preflight controls passed."
