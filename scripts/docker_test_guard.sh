#!/usr/bin/env bash
# Monitor a running Docker test container's Cargo target volume and free space.

# Measure the daemon filesystem, not DockerRootDir on a possibly different client host.
# Call after preparing the test image and before creating the Cargo target volume.
docker_test_available_kib() {
    local image="$1" platform="$2" available
    if ! available="$(docker run --rm --platform "$platform" --entrypoint sh "$image" \
        -c 'set -e; df -Pk / | awk "NR == 2 {print \$4}"')"; then
        echo "Cannot check the Docker daemon filesystem's free space." >&2
        return 1
    fi
    if [[ ! "$available" =~ ^[0-9]+$ ]]; then
        echo "Invalid Docker free-space measurement." >&2
        return 1
    fi
    printf '%s\n' "$available"
}

docker_test_guard() {
    local container_name="$1"
    local limit_marker="$2"
    local max_target_kib="$3"
    local min_free_kib="$4"
    local check_seconds="$5"
    local state usage target_kib free_kib

    while true; do
        state="$(docker inspect -f '{{.State.Running}}' "$container_name" 2>/dev/null || true)"
        case "$state" in
            true)
                if ! usage="$(docker exec "$container_name" sh -c \
                    'set -e; du -sk /cargo-target | awk "{print \$1}"; df -Pk /cargo-target | awk "NR == 2 {print \$4}"' \
                    2>/dev/null)"; then
                    if [ "$(docker inspect -f '{{.State.Running}}' "$container_name" 2>/dev/null || true)" != true ]; then
                        return
                    fi
                    echo "Cannot check Docker test disk usage; stopping the container." >&2
                    : > "$limit_marker"
                    docker stop -t 1 "$container_name" >/dev/null 2>&1 || true
                    return
                fi
                read -r target_kib free_kib <<< "${usage//$'\n'/ }"
                if [[ ! "$target_kib" =~ ^[0-9]+$ || ! "$free_kib" =~ ^[0-9]+$ ]]; then
                    echo "Invalid Docker test disk measurements; stopping the container." >&2
                    : > "$limit_marker"
                    docker stop -t 1 "$container_name" >/dev/null 2>&1 || true
                    return
                fi
                if (( target_kib > max_target_kib || free_kib < min_free_kib )); then
                    printf 'Docker test disk limit reached: target %.1f GiB, free %.1f GiB. Stopping the container.\n' \
                        "$(awk -v kib="$target_kib" 'BEGIN {print kib / 1048576}')" \
                        "$(awk -v kib="$free_kib" 'BEGIN {print kib / 1048576}')" >&2
                    : > "$limit_marker"
                    docker stop -t 1 "$container_name" >/dev/null 2>&1 || true
                    return
                fi
                sleep "$check_seconds"
                ;;
            false) return ;;
            *) sleep 1 ;;
        esac
    done
}
