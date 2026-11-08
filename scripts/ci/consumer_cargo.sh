#!/usr/bin/env bash
# Artifact consumers may inspect metadata or execute archives, but may not rebuild Rust.
set -euo pipefail
: "${ELEPHC_CI_BUILD_GUARD_LOG:?missing consumer build guard log}"
allowed=false
case "${1:-}" in
    --version|-V|metadata|locate-project) allowed=true ;;
    nextest)
        if [[ ${2:-} == run || ${2:-} == list ]]; then
            for argument in "$@"; do
                case "$argument" in --archive-file|--binaries-metadata) allowed=true ;; esac
            done
        fi
        ;;
esac
if [[ "$allowed" != true ]]; then
    printf 'Unexpected cargo %s in %s\n' "${1:-<empty>}" "$PWD" >> "$ELEPHC_CI_BUILD_GUARD_LOG"
    echo '::error::An artifact consumer attempted a Rust build. Add the missing configuration to its producer.' >&2
    exit 86
fi
: "${ELEPHC_CI_REAL_CARGO:?Cargo is unavailable for this metadata/archive command}"
exec "$ELEPHC_CI_REAL_CARGO" "$@"
