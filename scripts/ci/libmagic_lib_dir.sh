#!/bin/sh
# Print the `lib/` directory of the managed native libmagic artifact for one
# elephc target, so a shell step can export it as ELEPHC_FILEINFO_LIBMAGIC_LIB_DIR for
# `cargo test -p elephc-fileinfo` (crates/elephc-fileinfo/build.rs reads that variable, links
# libelephc_magic_database.a + libmagic.a from it, and compiles the libmagic-calling unit
# tests in behind cfg(elephc_fileinfo_native)).
#
# This is the shell twin of tests/codegen/support/fileinfo_native.rs: it reads the same
# durable cache the production resolver writes (ELEPHC_NATIVE_CACHE, else
# $XDG_CACHE_HOME/elephc/native, else $HOME/.cache/elephc/native), walks
#   artifacts/libmagic/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>/lib
# structurally, and prefers the (version, recipe) pinned by examples/fileinfo/elephc.lock —
# the same pin the codegen harness uses. A cache can accumulate newer siblings (5.48, r1),
# so falling back to the highest (version, recipe) keeps the command useful without a lock.
#
# POSIX sh on purpose: the Linux Docker test images are Alpine without bash.
#
# Usage: scripts/ci/libmagic_lib_dir.sh <elephc-target>   (e.g. linux-x86_64)
set -eu

if [ $# -ne 1 ] || [ -z "$1" ]; then
    echo "usage: $0 <elephc-target>" >&2
    exit 2
fi
target=$1

if [ -n "${ELEPHC_NATIVE_CACHE:-}" ]; then
    root=$ELEPHC_NATIVE_CACHE
elif [ -n "${XDG_CACHE_HOME:-}" ]; then
    root=$XDG_CACHE_HOME/elephc/native
else
    root=${HOME:?HOME is unset and no ELEPHC_NATIVE_CACHE/XDG_CACHE_HOME given}/.cache/elephc/native
fi
package_root=$root/artifacts/libmagic

if [ ! -d "$package_root" ]; then
    echo "error: no managed native libmagic artifact under $package_root" >&2
    echo "       (run: elephc native add libmagic --target $target)" >&2
    exit 1
fi

# Prefer the (version, recipe) pinned by the fileinfo example, the same pin the codegen
# harness reads. Fall back to the newest complete artifact when the lock is absent or the
# locked build was never installed.
CDPATH=
script_dir=$(cd "$(dirname "$0")" && pwd -P)
lock=$script_dir/../../examples/fileinfo/elephc.lock
locked_version=
locked_revision=
if [ -f "$lock" ]; then
    locked_version=$(awk -F'"' '
        /^\[\[package\]\]/ { found = 0 }
        /^name = / { found = ($0 == "name = \"libmagic\"") }
        found && /^version = "/ { print $2; exit }
    ' "$lock")
    locked_revision=$(awk -F' = ' '
        /^\[\[package\]\]/ { found = 0 }
        /^name = / { found = ($0 == "name = \"libmagic\"") }
        found && /^recipe = / { print $2; exit }
    ' "$lock")
fi

best=
if [ -n "$locked_version" ] && [ -n "$locked_revision" ]; then
    locked_root=$package_root/$locked_version/r$locked_revision
    if [ -d "$locked_root" ]; then
        best=$(
            find "$locked_root" -mindepth 5 -maxdepth 5 -type d -name lib \
                -path "$locked_root/*/$target/*/*/lib" 2>/dev/null \
            | while IFS= read -r lib; do
                [ -f "$lib/libelephc_magic_database.a" ] || continue
                [ -f "$lib/libmagic.a" ] || continue
                printf '%s\n' "$lib"
            done \
            | sort -r \
            | head -n 1
        )
    fi
fi

if [ -z "$best" ]; then
    # Exactly seven components below the package root, the fourth being the target.
    best=$(
        find "$package_root" -mindepth 7 -maxdepth 7 -type d -name lib \
            -path "$package_root/*/*/*/$target/*/*/lib" 2>/dev/null \
        | while IFS= read -r lib; do
            [ -f "$lib/libelephc_magic_database.a" ] || continue
            [ -f "$lib/libmagic.a" ] || continue
            rel=${lib#"$package_root"/}
            version=${rel%%/*}
            rest=${rel#*/}
            revision=${rest%%/*}
            revision=${revision#r}
            # Zero-padded numeric sort key: every dotted version part, then the revision.
            key=$(printf '%s' "$version" | awk -F. '{ for (i = 1; i <= NF; i++) printf "%08d", $i }')
            key="$key$(printf '%08d' "$revision" 2>/dev/null || printf '%08d' 0)"
            printf '%s\t%s\n' "$key" "$lib"
        done \
        | sort -r \
        | head -n 1 \
        | cut -f 2-
    )
fi

if [ -z "$best" ]; then
    echo "error: no libmagic artifact for $target holds both libelephc_magic_database.a and libmagic.a under $package_root" >&2
    echo "       (run: elephc native add libmagic --target $target)" >&2
    exit 1
fi

printf '%s\n' "$best"
