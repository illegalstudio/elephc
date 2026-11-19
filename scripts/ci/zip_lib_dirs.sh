#!/bin/sh
# Print the colon-separated `lib/` directories of the NEWEST managed native libzip
# artifact and its bzip2/LZMA/zlib/OpenSSL dependency chain for one elephc target,
# so a shell step can export ELEPHC_ZIP_NATIVE_LIB_DIRS for `cargo test -p elephc-zip`
# (crates/elephc-zip/build.rs links libzip.a + libbz2.a + liblzma.a + libz.a +
# libssl.a + libcrypto.a from these directories and compiles the libzip-calling
# unit tests in behind cfg(elephc_zip_native)).
#
# This is the shell twin of tests/codegen/support/zip_native.rs: it reads the same
# durable cache the production resolver writes (ELEPHC_NATIVE_CACHE, else
# $XDG_CACHE_HOME/elephc/native, else $HOME/.cache/elephc/native), walks
#   artifacts/<package>/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>/lib
# structurally, requires the exact archive set, and picks the highest (version,
# recipe revision) per package. Structural discovery is fine here because a
# mismatch can only fail a test link, never ship anything.
#
# The order IS the static link order: libzip, bzip2, xz, zlib, openssl.
#
# POSIX sh on purpose: the Linux Docker test images are Alpine without bash.
#
# Usage: scripts/ci/zip_lib_dirs.sh <elephc-target>   (e.g. linux-x86_64)
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
artifacts=$root/artifacts

# Prints the newest lib/ for one package that holds every named archive.
find_lib() {
    package=$1
    shift
    package_root=$artifacts/$package
    if [ ! -d "$package_root" ]; then
        return 1
    fi
    best=$(
        find "$package_root" -mindepth 7 -maxdepth 7 -type d -name lib \
            -path "$package_root/*/*/*/$target/*/*/lib" 2>/dev/null \
        | while IFS= read -r lib; do
            keep=1
            for archive in "$@"; do
                [ -f "$lib/$archive" ] || keep=0
            done
            [ "$keep" -eq 1 ] || continue
            rel=${lib#"$package_root"/}
            version=${rel%%/*}
            rest=${rel#*/}
            revision=${rest%%/*}
            revision=${revision#r}
            key=$(printf '%s' "$version" | awk -F. '{ for (i = 1; i <= NF; i++) printf "%08d", $i }')
            key="$key$(printf '%08d' "$revision" 2>/dev/null || printf '%08d' 0)"
            printf '%s\t%s\n' "$key" "$lib"
        done \
        | sort -r \
        | head -n 1 \
        | cut -f 2-
    )
    [ -n "$best" ] || return 1
    printf '%s\n' "$best"
}

dirs=
for spec in "libzip:libzip.a" "bzip2:libbz2.a" "xz:liblzma.a" "zlib:libz.a" "openssl:libssl.a libcrypto.a"; do
    package=${spec%%:*}
    archives=${spec#*:}
    # shellcheck disable=SC2086 # archive names are deliberately word-split.
    lib=$(find_lib "$package" $archives) || {
        echo "error: no $package artifact for $target holds: $archives" >&2
        echo "       (run: elephc native add libzip --target $target)" >&2
        exit 1
    }
    if [ -z "$dirs" ]; then
        dirs=$lib
    else
        dirs=$dirs:$lib
    fi
done

printf '%s\n' "$dirs"
