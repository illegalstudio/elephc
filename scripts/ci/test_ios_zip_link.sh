#!/usr/bin/env bash
# Builds the managed libzip chain and an exported zip-using PHP staticlib for one iOS
# target, then performs the final application-host link. The binary is not run: the
# device row needs signing/provisioning, and a runner need not have a Simulator
# runtime installed. Mirrors test_ios_xml_link.sh.

set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <elephc-target> <rust-target> <sdk> <clang-target>" >&2
  exit 2
fi

ELEPHC_TARGET="$1"
RUST_TARGET="$2"
APPLE_SDK="$3"
CLANG_TARGET="$4"

case "$ELEPHC_TARGET:$RUST_TARGET:$APPLE_SDK:$CLANG_TARGET" in
  ios-arm64:aarch64-apple-ios:iphoneos:arm64-apple-ios13.0) ;;
  ios-sim-arm64:aarch64-apple-ios-sim:iphonesimulator:arm64-apple-ios13.0-simulator) ;;
  *)
    echo "unsupported iOS zip link tuple: $ELEPHC_TARGET/$RUST_TARGET/$APPLE_SDK/$CLANG_TARGET" >&2
    exit 2
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures/ios-zip-link"
ELEPHC_BIN="$PROJECT_DIR/target/debug/elephc"
NATIVE_CACHE="${ELEPHC_NATIVE_CACHE:?ELEPHC_NATIVE_CACHE must name the isolated CI cache}"
SDK_PATH="$(xcrun --sdk "$APPLE_SDK" --show-sdk-path)"

# Wrapper paths are stable across CI runs so the managed-native toolchain fingerprint can
# reuse the actions/cache entry. Their contents bake in the SDK and target because recipe
# commands run with a scrubbed environment (src/native_deps/toolchain.rs).
TOOLCHAIN_DIR="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-toolchain/$ELEPHC_TARGET"
mkdir -p "$TOOLCHAIN_DIR"
CC_WRAPPER="$TOOLCHAIN_DIR/cc"
AR_WRAPPER="$TOOLCHAIN_DIR/ar"
RANLIB_WRAPPER="$TOOLCHAIN_DIR/ranlib"
XCRUN_BIN="$(command -v xcrun)"
AR_BIN="$(xcrun --sdk "$APPLE_SDK" --find ar)"
RANLIB_BIN="$(xcrun --sdk "$APPLE_SDK" --find ranlib)"

printf '#!/usr/bin/env bash\nexec %q --sdk %q clang -target %q -isysroot %q "$@"\n' \
  "$XCRUN_BIN" "$APPLE_SDK" "$CLANG_TARGET" "$SDK_PATH" > "$CC_WRAPPER"
printf '#!/usr/bin/env bash\nexec %q "$@"\n' "$AR_BIN" > "$AR_WRAPPER"
printf '#!/usr/bin/env bash\nexec %q "$@"\n' "$RANLIB_BIN" > "$RANLIB_WRAPPER"
chmod +x "$CC_WRAPPER" "$AR_WRAPPER" "$RANLIB_WRAPPER"

TARGET_ENV="$(printf '%s' "$ELEPHC_TARGET" | tr '[:lower:]-' '[:upper:]_')"
export "ELEPHC_NATIVE_CC_${TARGET_ENV}=$CC_WRAPPER"
export "ELEPHC_NATIVE_AR_${TARGET_ENV}=$AR_WRAPPER"
export "ELEPHC_NATIVE_RANLIB_${TARGET_ENV}=$RANLIB_WRAPPER"

echo "==> building compiler and materializing libzip for $ELEPHC_TARGET"
if [[ ${ELEPHC_CI_PREBUILT_COMPILER:-0} == 1 ]]; then
    test -x "$PROJECT_DIR/target/debug/elephc"
else
    (cd "$PROJECT_DIR" && cargo build --bin elephc)
fi
"$ELEPHC_BIN" native install --locked \
  --target "$ELEPHC_TARGET" \
  --manifest-path "$PROJECT_DIR/examples/zip/elephc.toml"

# The zip prelude registers a `zip://` stream wrapper and calls fopen(), which switches
# on the runtime's phar stream feature: every program linking the zip bridge references
# `elephc_phar_*`, exactly as the Linux managed-native smoke documents. Neither bridge
# is whole-archived, so both are plain archive inputs below.
echo "==> cross-building the Rust zip and phar bridges for $RUST_TARGET"
if [[ ${ELEPHC_CI_PREBUILT_IOS_BRIDGES:-0} == 1 ]]; then
    test -s "$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_zip.a"
    test -s "$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"
else
    (cd "$PROJECT_DIR" && cargo build -p elephc-zip -p elephc-phar --target "$RUST_TARGET")
fi
BRIDGE_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_zip.a"
PHAR_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"
test -s "$BRIDGE_ARCHIVE"
test -s "$PHAR_ARCHIVE"

# Read the pinned version/recipe for each package from the lock instead of hardcoding
# them, so a recipe bump cannot leave this job looking at an older artifact directory.
ZIP_LOCK="$PROJECT_DIR/examples/zip/elephc.lock"
lock_meta() {
  python3 - "$ZIP_LOCK" "$1" "$2" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
package, field = sys.argv[2], sys.argv[3]
for block in text.split("[[package]]")[1:]:
    name = re.search(r'^name = "([^"]+)"$', block, re.M)
    if name is None or name.group(1) != package:
        continue
    value = re.search(rf'^{re.escape(field)} = (.+)$', block, re.M)
    if value is None:
        break
    print(value.group(1).strip().strip('"'))
    break
PY
}

LIBZIP_VERSION="$(lock_meta libzip version)"
LIBZIP_RECIPE="$(lock_meta libzip recipe)"
BZIP2_VERSION="$(lock_meta bzip2 version)"
BZIP2_RECIPE="$(lock_meta bzip2 recipe)"
XZ_VERSION="$(lock_meta xz version)"
XZ_RECIPE="$(lock_meta xz recipe)"
ZLIB_VERSION="$(lock_meta zlib version)"
ZLIB_RECIPE="$(lock_meta zlib recipe)"
OPENSSL_VERSION="$(lock_meta openssl version)"
OPENSSL_RECIPE="$(lock_meta openssl recipe)"
for value in "$LIBZIP_VERSION" "$LIBZIP_RECIPE" "$BZIP2_VERSION" "$BZIP2_RECIPE" \
  "$XZ_VERSION" "$XZ_RECIPE" "$ZLIB_VERSION" "$ZLIB_RECIPE" \
  "$OPENSSL_VERSION" "$OPENSSL_RECIPE"; do
  test -n "$value"
done

# Select an installed archive for this exact package recipe and target. More than one
# toolchain fingerprint can remain after an Xcode update; every candidate has already
# passed receipt verification during `native install`, and the source/version/recipe and
# target path components below keep incompatible artifacts out.
find_native_archive() {
  local package="$1"
  local version="$2"
  local recipe="$3"
  local archive="$4"
  local base="$NATIVE_CACHE/artifacts/$package/$version/r$recipe"
  local found
  found="$(find "$base" -type f -path "*/$ELEPHC_TARGET/*/lib/$archive" -print 2>/dev/null | sort | tail -n 1)"
  if [ -z "$found" ] || [ ! -s "$found" ]; then
    echo "missing verified $package archive $archive for $ELEPHC_TARGET under $base" >&2
    exit 1
  fi
  printf '%s\n' "$found"
}

# libzip's dependency order is its link order: libzip, then bzip2, xz, zlib, and the
# two OpenSSL archives (src/linker/native_packages.rs and the native catalogue).
LIBZIP_ARCHIVE="$(find_native_archive libzip "$LIBZIP_VERSION" "$LIBZIP_RECIPE" libzip.a)"
BZIP2_ARCHIVE="$(find_native_archive bzip2 "$BZIP2_VERSION" "$BZIP2_RECIPE" libbz2.a)"
LZMA_ARCHIVE="$(find_native_archive xz "$XZ_VERSION" "$XZ_RECIPE" liblzma.a)"
ZLIB_ARCHIVE="$(find_native_archive zlib "$ZLIB_VERSION" "$ZLIB_RECIPE" libz.a)"
LIBSSL_ARCHIVE="$(find_native_archive openssl "$OPENSSL_VERSION" "$OPENSSL_RECIPE" libssl.a)"
LIBCRYPTO_ARCHIVE="$(find_native_archive openssl "$OPENSSL_VERSION" "$OPENSSL_RECIPE" libcrypto.a)"

WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-zip-link.XXXXXX")"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

cp "$FIXTURE_DIR/main.php" "$FIXTURE_DIR/host.c" "$WORK_DIR/"
cp "$PROJECT_DIR/examples/zip/elephc.toml" "$WORK_DIR/elephc.toml"
cp "$PROJECT_DIR/examples/zip/elephc.lock" "$WORK_DIR/elephc.lock"

# `--with-zip` is redundant for a program that already names the surface; it is passed
# anyway so the bridge is forced even if detection ever changed.
echo "==> compiling zip-using PHP as an $ELEPHC_TARGET staticlib"
"$ELEPHC_BIN" build --with-zip --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
test -s "$WORK_DIR/libmain.a"
test -s "$WORK_DIR/libmain.h"

# The staticlib must actually depend on the bridge, or a green link proves nothing.
nm -u "$WORK_DIR/libmain.a" > "$WORK_DIR/main.undefined"
grep -F "elephc_zip_open" "$WORK_DIR/main.undefined"
grep -F "elephc_zip_get_index" "$WORK_DIR/main.undefined"
grep -F "elephc_zip_close" "$WORK_DIR/main.undefined"

echo "==> linking the complete iOS host/native/bridge graph"
xcrun --sdk "$APPLE_SDK" clang \
  -target "$CLANG_TARGET" \
  -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" \
  "$WORK_DIR/host.c" \
  "$WORK_DIR/libmain.a" \
  "$BRIDGE_ARCHIVE" \
  "$PHAR_ARCHIVE" \
  "$LIBZIP_ARCHIVE" \
  "$BZIP2_ARCHIVE" \
  "$LZMA_ARCHIVE" \
  "$ZLIB_ARCHIVE" \
  "$LIBSSL_ARCHIVE" \
  "$LIBCRYPTO_ARCHIVE" \
  -o "$WORK_DIR/ios-zip-host"

test -s "$WORK_DIR/ios-zip-host"
xcrun vtool -show-build "$WORK_DIR/ios-zip-host"
echo "managed libzip zip compile/link succeeded for $ELEPHC_TARGET"
