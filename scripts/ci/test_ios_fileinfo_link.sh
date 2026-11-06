#!/usr/bin/env bash

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
    echo "unsupported iOS fileinfo link tuple: $ELEPHC_TARGET/$RUST_TARGET/$APPLE_SDK/$CLANG_TARGET" >&2
    exit 2
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures/ios-fileinfo-link"
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

echo "==> building compiler and materializing libmagic for $ELEPHC_TARGET"
(cd "$PROJECT_DIR" && cargo build --bin elephc)
"$ELEPHC_BIN" native install --locked \
  --target "$ELEPHC_TARGET" \
  --manifest-path "$PROJECT_DIR/examples/fileinfo/elephc.toml"

echo "==> cross-building the Rust fileinfo, phar and TLS bridges for $RUST_TARGET"
(cd "$PROJECT_DIR" && cargo build -p elephc-fileinfo -p elephc-phar -p elephc-tls --target "$RUST_TARGET")
BRIDGE_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_fileinfo.a"
PHAR_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"
TLS_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_tls.a"
test -s "$BRIDGE_ARCHIVE"
test -s "$PHAR_ARCHIVE"
test -s "$TLS_ARCHIVE"

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

SHIM_ARCHIVE="$(find_native_archive libmagic 5.46 1 libelephc_magic_database.a)"
LIBMAGIC_ARCHIVE="$(find_native_archive libmagic 5.46 1 libmagic.a)"

WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-fileinfo-link.XXXXXX")"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

nm "$SHIM_ARCHIVE" > "$WORK_DIR/database.symbols"
grep -F "elephc_magic_load_default" "$WORK_DIR/database.symbols"

cp "$FIXTURE_DIR/main.php" "$FIXTURE_DIR/host.c" "$WORK_DIR/"
cp "$PROJECT_DIR/examples/fileinfo/elephc.toml" "$WORK_DIR/elephc.toml"
cp "$PROJECT_DIR/examples/fileinfo/elephc.lock" "$WORK_DIR/elephc.lock"

echo "==> compiling fileinfo-using PHP as an $ELEPHC_TARGET staticlib"
"$ELEPHC_BIN" --with-fileinfo --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
test -s "$WORK_DIR/libmain.a"
test -s "$WORK_DIR/libmain.h"

nm -u "$WORK_DIR/libmain.a" > "$WORK_DIR/main.undefined"
grep -F "elephc_fileinfo_open" "$WORK_DIR/main.undefined"
grep -F "elephc_phar_stream_open_url" "$WORK_DIR/main.undefined"

echo "==> linking the complete iOS host/native/bridge graph"
xcrun --sdk "$APPLE_SDK" clang \
  -target "$CLANG_TARGET" \
  -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" \
  "$WORK_DIR/host.c" \
  "$WORK_DIR/libmain.a" \
  "$BRIDGE_ARCHIVE" \
  "$PHAR_ARCHIVE" \
  "$TLS_ARCHIVE" \
  "$SHIM_ARCHIVE" \
  "$LIBMAGIC_ARCHIVE" \
  -lz \
  -lbz2 \
  -o "$WORK_DIR/ios-fileinfo-host"

test -s "$WORK_DIR/ios-fileinfo-host"
xcrun vtool -show-build "$WORK_DIR/ios-fileinfo-host"
echo "libmagic fileinfo compile/link succeeded for $ELEPHC_TARGET"
