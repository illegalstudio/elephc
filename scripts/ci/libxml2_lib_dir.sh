#!/bin/sh
# Print the `lib/` directory of the NEWEST managed native libxml2 artifact for one
# elephc target, so a shell step can export it as ELEPHC_XML_LIBXML2_LIB_DIR for
# `cargo test -p elephc-xml` (crates/elephc-xml/build.rs reads that variable, links
# libelephc_libxml2_shim.a + libxml2.a from it, and compiles the libxml2-calling unit
# tests in behind cfg(elephc_xml_native)).
#
# This is the shell twin of tests/codegen/support/xml_native.rs: it reads the same
# durable cache the production resolver writes (ELEPHC_NATIVE_CACHE, else
# $XDG_CACHE_HOME/elephc/native, else $HOME/.cache/elephc/native), walks
#   artifacts/libxml2/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>/lib
# structurally, requires BOTH archives, and picks the highest (version, recipe
# revision) -- a cache accumulates siblings across catalog bumps and directory order
# is unspecified, so "first match" would be a coin toss. Structural discovery is
# fine here because a mismatch can only fail a test link, never ship anything.
#
# POSIX sh on purpose: the Linux Docker test images are Alpine without bash.
#
# Usage: scripts/ci/libxml2_lib_dir.sh <elephc-target>   (e.g. linux-x86_64)
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
package_root=$root/artifacts/libxml2

if [ ! -d "$package_root" ]; then
    echo "error: no managed native libxml2 artifact under $package_root" >&2
    echo "       (run: elephc native add libxml2 --target $target)" >&2
    exit 1
fi

# Exactly seven components below the package root, the fourth being the target.
best=$(
    find "$package_root" -mindepth 7 -maxdepth 7 -type d -name lib \
        -path "$package_root/*/*/*/$target/*/*/lib" 2>/dev/null \
    | while IFS= read -r lib; do
        [ -f "$lib/libelephc_libxml2_shim.a" ] || continue
        [ -f "$lib/libxml2.a" ] || continue
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

if [ -z "$best" ]; then
    echo "error: no libxml2 artifact for $target holds both libelephc_libxml2_shim.a and libxml2.a under $package_root" >&2
    echo "       (run: elephc native add libxml2 --target $target)" >&2
    exit 1
fi

printf '%s\n' "$best"
