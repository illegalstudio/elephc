#!/usr/bin/env bash
#
# Verifies that one packaged elephc tarball is complete, by asking the compiler
# inside it what it needs and then holding it to the answer.
#
# A bridge archive is resolved from the directory the binary lives in, or from
# its sibling `lib/`, so a compiler ships with exactly what was packed beside
# it — and it will happily advertise a capability whose archive nobody packed.
# That is not a hypothetical: released tarballs carried no
# `libelephc_magician.a` from 0.26.3 to 0.26.5, so an installed elephc refused
# `--with-eval` and every `eval()` it could not fold at compile time. Nothing
# that runs inside a checkout can notice, because `target/` always holds every
# archive; only the shipped artifact is short.
#
# The list of things to check is not written here. It comes from
# `elephc --print-capabilities`, which is a projection of the bridge table
# inside the binary under test — so a bridge added to `BRIDGES` is carried into
# this check by the edit that declares it, and there is no second catalog for
# anyone to forget to update.
#
# Usage: scripts/verify-release-artifact.sh <tarball>
#
# Focused fixture (mock packaged binary, no cargo suite):
#   cargo test --test verify_release_artifact
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "usage: $0 <tarball>" >&2
    exit 2
fi

TARBALL="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
if [ ! -s "$TARBALL" ]; then
    echo "error: $TARBALL is missing or empty" >&2
    exit 2
fi

# Run entirely outside any checkout. A bridge that cannot be found is otherwise
# auto-built from a workspace the compiler locates on its own, which would let a
# tarball missing an archive pass by compiling the archive on the spot.
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT
cd "$WORKDIR"

# Same reason: an inherited override would point the compiler at archives that
# are not the ones being verified.
while IFS='=' read -r name _; do
    case "$name" in
        ELEPHC_*_LIB_DIR) unset "$name" ;;
    esac
done < <(env)

mkdir -p extract prefix/bin prefix/lib
tar xzf "$TARBALL" -C extract

# The sibling `lib/` layout, which is what Homebrew installs and the less
# obvious of the two resolution paths.
mv extract/elephc prefix/bin/elephc
chmod +x prefix/bin/elephc
shopt -s nullglob
for archive in extract/*.a; do
    mv "$archive" prefix/lib/
done
shopt -u nullglob

ELEPHC="$WORKDIR/prefix/bin/elephc"
echo "Verifying $(basename "$TARBALL")"
echo "  $("$ELEPHC" --version)"
echo

printf '<?php echo "ok";' > probe.php

FAILURES=0
CHECKED=0

fail() {
    echo "  FAIL  $*"
    FAILURES=$((FAILURES + 1))
}

# `--print-capabilities` prints `<kind>\t<name>[\t<archive>...]`.
CAPABILITIES="$("$ELEPHC" --print-capabilities)"
if [ -z "$CAPABILITIES" ]; then
    echo "error: the packaged binary reported no capabilities at all" >&2
    exit 1
fi

DECLARED=""
while IFS=$'\t' read -r kind name archives; do
    [ -n "$name" ] || continue

    # Every archive the binary names must have been packed beside it.
    missing=""
    for archive in $archives; do
        DECLARED="$DECLARED $archive"
        if [ ! -s "prefix/lib/$archive" ]; then
            missing="$missing $archive"
        fi
    done
    if [ -n "$missing" ]; then
        fail "$kind $name: not packed:$missing"
        continue
    fi

    # Presence is not the same as linkability: an archive can be truncated, or
    # built for the wrong architecture, and both fail at the link rather than
    # at `test -s`. Only the capabilities that name an archive are link-tested,
    # because those are the ones this tarball is responsible for — `regex`
    # resolves a managed native package and `mysqli` links the `pdo` archive
    # already covered by its own line.
    #
    # Some packed bridges need managed native packages when force-enabled.
    # Prepare curl's dependency chain, mbstring's Oniguruma and optional MIME
    # providers, and XML's libxml2 provider before their probes.
    # Each bridge must still compile and link once its requirements are present.
    #
    # The check ends at the link. Producing the executable is what proves the
    # archive was packed and usable, and it is the whole of what packaging can
    # get wrong; whether the program then behaves is what CI compiles and runs
    # thousands of times. Asserting on its output would also need a per-
    # capability exception the moment one is added that does not print and
    # exit — `--with-web` builds a prefork HTTP server — and a list of
    # exceptions is the kind of catalog this probe exists to avoid.
    if [ -z "$archives" ]; then
        echo "  skip  $kind $name (needs no archive from this tarball)"
        continue
    fi

    CHECKED=$((CHECKED + 1))
    rm -f probe
    native_packages=""
    case "$name" in
        curl) native_packages="curl" ;;
        mbstring) native_packages="oniguruma pcre2" ;;
    esac
    native_ready=true
    for native_package in $native_packages; do
        echo "  ...   $kind $name: adding managed native package $native_package before --with-$name"
        if ! "$ELEPHC" native add "$native_package" >native-add.log 2>&1; then
            fail "$kind $name: native add $native_package failed"
            sed 's/^/          /' native-add.log
            native_ready=false
            break
        fi
        sed 's/^/          /' native-add.log
    done
    if [ "$native_ready" = false ]; then
        continue
    fi
    if [ "$name" = "xml" ]; then
        echo "  ...   $kind $name: adding managed native package libxml2 before --with-xml"
        if ! "$ELEPHC" native add libxml2 >native-add.log 2>&1; then
            fail "$kind $name: native add libxml2 failed"
            sed 's/^/          /' native-add.log
            continue
        fi
        sed 's/^/          /' native-add.log
    fi
    if ! "$ELEPHC" "--with-$name" probe.php >compile.log 2>&1; then
        fail "$kind $name: --with-$name did not link"
        sed 's/^/          /' compile.log
        continue
    fi
    if [ ! -x probe ]; then
        fail "$kind $name: --with-$name reported success but produced no executable"
        continue
    fi
    echo "  ok    $kind $name ($(echo "$archives" | tr '\t' ' '))"
done <<< "$CAPABILITIES"

# The reverse direction: an archive shipped that nothing in the binary asks for
# is dead weight, and usually means a bridge was renamed and the packing list
# kept the old name. Reported rather than failed — it ships a working compiler.
shopt -s nullglob
for archive in prefix/lib/*.a; do
    base="$(basename "$archive")"
    case " $DECLARED " in
        *" $base "*) ;;
        *) echo "  warn  $base is packed but no capability asks for it" ;;
    esac
done
shopt -u nullglob

echo
if [ "$FAILURES" -ne 0 ]; then
    echo "$FAILURES capability check(s) failed for $(basename "$TARBALL")"
    exit 1
fi
echo "All $CHECKED linkable capabilities verified for $(basename "$TARBALL")"
