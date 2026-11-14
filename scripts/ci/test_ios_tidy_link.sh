#!/usr/bin/env bash
# Builds the pinned managed libtidy package and an exported tidy-using PHP staticlib
# for one iOS target, then performs the final application-host link. The binary is not
# run: the device row needs signing/provisioning and the simulator runner need not have
# a runtime. Mirrors test_ios_gmp_link.sh.

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
    echo "unsupported iOS tidy link tuple: $ELEPHC_TARGET/$RUST_TARGET/$APPLE_SDK/$CLANG_TARGET" >&2
    exit 2
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures/ios-tidy-link"
ELEPHC_BIN="$PROJECT_DIR/target/debug/elephc"
NATIVE_CACHE="${ELEPHC_NATIVE_CACHE:?ELEPHC_NATIVE_CACHE must name the isolated CI cache}"
SDK_PATH="$(xcrun --sdk "$APPLE_SDK" --show-sdk-path)"

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

echo "==> building compiler and materializing libtidy for $ELEPHC_TARGET"
if [[ ${ELEPHC_CI_PREBUILT_COMPILER:-0} == 1 ]]; then
    test -x "$PROJECT_DIR/target/debug/elephc"
else
    (cd "$PROJECT_DIR" && cargo build --bin elephc)
fi
"$ELEPHC_BIN" native install --locked \
  --target "$ELEPHC_TARGET" \
  --manifest-path "$PROJECT_DIR/examples/tidy/elephc.toml"

echo "==> cross-building the Rust tidy and phar bridges for $RUST_TARGET"
if [[ ${ELEPHC_CI_PREBUILT_IOS_BRIDGES:-0} == 1 ]]; then
    test -s "$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_tidy.a"
    test -s "$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"
else
    (cd "$PROJECT_DIR" && cargo build -p elephc-tidy -p elephc-phar --target "$RUST_TARGET")
fi
BRIDGE_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_tidy.a"
PHAR_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"
test -s "$BRIDGE_ARCHIVE"
test -s "$PHAR_ARCHIVE"

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

LIBTIDY_ARCHIVE="$(find_native_archive libtidy 5.8.0 1 libtidy.a)"

WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-tidy-link.XXXXXX")"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

cp "$FIXTURE_DIR/main.php" "$FIXTURE_DIR/host.c" "$WORK_DIR/"
cp "$PROJECT_DIR/examples/tidy/elephc.toml" "$PROJECT_DIR/examples/tidy/elephc.lock" "$WORK_DIR/"

echo "==> compiling tidy-using PHP as an $ELEPHC_TARGET staticlib"
"$ELEPHC_BIN" --with-tidy --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
test -s "$WORK_DIR/libmain.a"
test -s "$WORK_DIR/libmain.h"

nm -u "$WORK_DIR/libmain.a" > "$WORK_DIR/main.undefined"
grep -F "elephc_tidy_create" "$WORK_DIR/main.undefined"
grep -F "elephc_tidy_parse" "$WORK_DIR/main.undefined"
# The tidy prelude's file reads go through the runtime's stream layer, which
# references the phar bridge and the SDK's zlib/bzip2 exactly as fileinfo's do.
grep -F "elephc_phar_extract_url" "$WORK_DIR/main.undefined"

echo "==> linking the complete iOS host/native/bridge graph"
xcrun --sdk "$APPLE_SDK" clang \
  -target "$CLANG_TARGET" \
  -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" \
  "$WORK_DIR/host.c" \
  "$WORK_DIR/libmain.a" \
  "$BRIDGE_ARCHIVE" \
  "$PHAR_ARCHIVE" \
  "$LIBTIDY_ARCHIVE" \
  -lz \
  -lbz2 \
  -o "$WORK_DIR/ios-tidy-host"

test -s "$WORK_DIR/ios-tidy-host"
xcrun vtool -show-build "$WORK_DIR/ios-tidy-host"
echo "libtidy tidy compile/link succeeded for $ELEPHC_TARGET"
