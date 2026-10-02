#!/usr/bin/env bash
# Regenerates src/native_deps/php_src_headers.rs from a php-src release tarball.
#
# Every header under Zend/, main/, TSRM/ and ext/ is retained, except those in
# test directories (one is empty, which materialize refuses), plus the three
# configure-generated headers Elephc writes itself. The list must be exhaustive:
# materialize refuses a staging tree holding any file the catalog did not name.
#
# Usage: scripts/php-ext/gen_php_src_headers.sh <tarball> [php-X.Y.Z] > src/native_deps/php_src_headers.rs
set -euo pipefail

tarball="$1"
version="${2:-$(basename "$tarball" .tar.gz)}"
listing="$(mktemp)"
trap 'rm -f "$listing"' EXIT

{
  tar tzf "$tarball" \
    | sed -n -E 's#^[^/]+/((Zend|main|TSRM|ext)/.*\.h)$#include/\1#p' \
    | grep -v '/tests/'
  printf '%s\n' include/Zend/zend_config.h include/main/php_config.h include/main/build-defs.h
} | LC_ALL=C sort -u > "$listing"

echo "//! Every PHP header the \`php-src\` catalog package retains."
echo
echo "// GENERATED from ${version} by scripts/php-ext/gen_php_src_headers.sh — do not edit by hand."
echo "pub const RETAINED_HEADERS: &[&str] = &["
sed 's/.*/    "&",/' "$listing"
echo "];"
