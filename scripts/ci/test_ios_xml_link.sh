#!/usr/bin/env bash
# Builds the pinned managed libxml2 package and an exported xml-using PHP staticlib (SAX
# parser with handlers plus XMLWriter) for one iOS target, then performs the final
# application-host link. The binary is not run: the device row needs signing/provisioning
# and the simulator runner need not have a runtime. Mirrors test_ios_curl_link.sh.

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
    echo "unsupported iOS xml link tuple: $ELEPHC_TARGET/$RUST_TARGET/$APPLE_SDK/$CLANG_TARGET" >&2
    exit 2
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures/ios-xml-link"
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

echo "==> building compiler and materializing libxml2 for $ELEPHC_TARGET"
(cd "$PROJECT_DIR" && cargo build --bin elephc)
"$ELEPHC_BIN" native install --locked \
  --target "$ELEPHC_TARGET" \
  --manifest-path "$PROJECT_DIR/examples/xml/elephc.toml"

# Two Rust bridges take part in the host link. `elephc_xml` is the one under test; the
# xml prelude also calls fopen(), which switches on the runtime's phar stream feature,
# so every program linking the xml bridge references `elephc_phar_*` (plus the SDK's
# zlib/bzip2) exactly as the Linux managed-native smoke documents. Neither bridge is
# whole-archived, so both are plain archive inputs below.
echo "==> cross-building the Rust xml and phar bridges for $RUST_TARGET"
(cd "$PROJECT_DIR" && cargo build -p elephc-xml -p elephc-phar --target "$RUST_TARGET")
BRIDGE_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_xml.a"
PHAR_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"
test -s "$BRIDGE_ARCHIVE"
test -s "$PHAR_ARCHIVE"

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

# Shim first: it reaches into libxml2's parser-context structs on the bridge's behalf,
# so it must precede libxml2.a (src/linker/bridges.rs LIBXML2_ARCHIVES).
SHIM_ARCHIVE="$(find_native_archive libxml2 2.15.3 1 libelephc_libxml2_shim.a)"
LIBXML2_ARCHIVE="$(find_native_archive libxml2 2.15.3 1 libxml2.a)"

WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-xml-link.XXXXXX")"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# Compile-time evidence that this is the `--with-iconv` build the recipe pins: libxml2's
# encoding handlers must reach the platform iconv, which every Apple SDK ships as a
# separate `libiconv.tbd` and the host link below names as `-liconv`. An archive built
# without iconv would link just as well while decoding nothing beyond UTF-8/Latin-1.
nm -u "$LIBXML2_ARCHIVE" > "$WORK_DIR/libxml2.undefined"
grep -F "iconv_open" "$WORK_DIR/libxml2.undefined"
nm "$SHIM_ARCHIVE" > "$WORK_DIR/shim.symbols"
grep -F "elephc_libxml2_v1_parser_create" "$WORK_DIR/shim.symbols"

cp "$FIXTURE_DIR/main.php" "$FIXTURE_DIR/host.c" "$WORK_DIR/"
cp "$PROJECT_DIR/examples/xml/elephc.toml" "$WORK_DIR/elephc.toml"
cp "$PROJECT_DIR/examples/xml/elephc.lock" "$WORK_DIR/elephc.lock"

# `--with-xml` is redundant for a program that already names the surface; it is passed
# anyway so the bridge is forced even if detection ever changed.
echo "==> compiling xml-using PHP as an $ELEPHC_TARGET staticlib"
"$ELEPHC_BIN" --with-xml --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
test -s "$WORK_DIR/libmain.a"
test -s "$WORK_DIR/libmain.h"

# The staticlib must actually depend on both halves of the bridge and on the phar
# stream feature the link line below satisfies; otherwise a green link proves nothing.
nm -u "$WORK_DIR/libmain.a" > "$WORK_DIR/main.undefined"
grep -F "elephc_xml_parser_create" "$WORK_DIR/main.undefined"
grep -F "elephc_xml_writer_create" "$WORK_DIR/main.undefined"
grep -F "elephc_phar_stream_open_url" "$WORK_DIR/main.undefined"

# Same shape as the compiler's own Apple link plan for this program: the exported
# library, the bridges, the managed archives (shim before libxml2), then the SDK
# libraries — `-liconv` for libxml2's encoding handlers, `-lz -lbz2` for the runtime's
# phar stream layer — trailing every archive so a left-to-right scan resolves them.
echo "==> linking the complete iOS host/native/bridge graph"
xcrun --sdk "$APPLE_SDK" clang \
  -target "$CLANG_TARGET" \
  -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" \
  "$WORK_DIR/host.c" \
  "$WORK_DIR/libmain.a" \
  "$BRIDGE_ARCHIVE" \
  "$PHAR_ARCHIVE" \
  "$SHIM_ARCHIVE" \
  "$LIBXML2_ARCHIVE" \
  -liconv \
  -lz \
  -lbz2 \
  -o "$WORK_DIR/ios-xml-host"

test -s "$WORK_DIR/ios-xml-host"
xcrun vtool -show-build "$WORK_DIR/ios-xml-host"
echo "libxml2 xml compile/link succeeded for $ELEPHC_TARGET"
