#!/bin/sh
# Install a pinned CMake on a Linux runner that does not ship one.
#
# The managed libzip recipe is CMake-only, and the published CI images do not
# gain cmake until their next main-branch rollover. Runtime package installation
# is unavailable inside those containers (the runner user cannot write apt's
# state), so fetch the official Kitware binary tarball into RUNNER_TEMP instead,
# verify its pinned SHA-256, and publish its bin/ through GITHUB_PATH.
#
# The selected PATH participates in the native toolchain fingerprint, so producer
# and consumer jobs must expose CMake from this same RUNNER_TEMP directory.
#
# Usage: sh scripts/ci/ensure_cmake.sh
set -eu

print_path=0
if [ "${1:-}" = "--print-path" ]; then
    print_path=1
fi

if command -v cmake >/dev/null 2>&1; then
    if [ "$print_path" -eq 1 ]; then
        directory="$(dirname "$(command -v cmake)")"
        case ":$PATH:" in
            *":$directory:"*) ;;
            *) printf '%s\n' "$directory" ;;
        esac
    fi
    exit 0
fi

version=3.28.3
case "$(uname -m)" in
    x86_64)
        arch=x86_64
        sha=804d231460ab3c8b556a42d2660af4ac7a0e21c98a7f8ee3318a74b4a9a187a6
        ;;
    aarch64|arm64)
        arch=aarch64
        sha=bbf023139f944cefe731d944f2864d8ea3ea0c4f9310b46ac72b3cb4e314b023
        ;;
    *)
        echo "ensure_cmake: unsupported architecture $(uname -m)" >&2
        exit 2
        ;;
esac

root=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
mkdir -p "$root"
directory="$root/cmake-$version-linux-$arch"
if [ ! -x "$directory/bin/cmake" ]; then
    archive="$root/cmake-$version-linux-$arch.tar.gz"
    curl -fsSL \
        "https://github.com/Kitware/CMake/releases/download/v$version/cmake-$version-linux-$arch.tar.gz" \
        -o "$archive"
    if command -v sha256sum >/dev/null 2>&1; then
        printf '%s  %s\n' "$sha" "$archive" | sha256sum -c - >/dev/null
    else
        printf '%s  %s\n' "$sha" "$archive" | shasum -a 256 -c - >/dev/null
    fi
    tar -xzf "$archive" -C "$root"
fi

bin="$directory/bin"
if [ "$print_path" -eq 1 ]; then
    printf '%s\n' "$bin"
    exit 0
fi
if [ -n "${GITHUB_PATH:-}" ]; then
    printf '%s\n' "$bin" >> "$GITHUB_PATH"
fi
# The container jobs run some later steps through the custom `elephc-ci-shell`,
# which does not necessarily consume GITHUB_PATH; export PATH explicitly too.
if [ -n "${GITHUB_ENV:-}" ]; then
    printf 'PATH=%s:%s\n' "$bin" "$PATH" >> "$GITHUB_ENV"
fi
printf 'ensure_cmake: cmake %s from %s\n' "$version" "$bin"
