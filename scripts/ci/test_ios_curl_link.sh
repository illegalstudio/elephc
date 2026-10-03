#!/usr/bin/env bash
# Builds the pinned native curl chain and an exported curl-using PHP staticlib for one
# iOS target, then performs the final application-host link.
#
# By default the binary is NOT run: the device row needs signing/provisioning, and a
# runner need not have a Simulator runtime installed.
#
# `--run` adds the live half (issue #873). Compile/link evidence proves libcurl carries
# AppleSecTrust and that the Security framework resolves; it cannot prove a request
# actually verifies against the iOS trust store, because default iOS HTTPS leaves
# CURLOPT_CAINFO unset so SecTrust supplies the anchors. A regression that compiles,
# links, and still fails TLS is invisible without a transfer. Simulator only.

set -euo pipefail

if [ "$#" -lt 4 ] || [ "$#" -gt 5 ]; then
  echo "usage: $0 <elephc-target> <rust-target> <sdk> <clang-target> [--run]" >&2
  exit 2
fi

ELEPHC_TARGET="$1"
RUST_TARGET="$2"
APPLE_SDK="$3"
CLANG_TARGET="$4"
RUN_LIVE=0
if [ "$#" -eq 5 ]; then
  if [ "$5" != "--run" ]; then
    echo "unknown option: $5 (expected --run)" >&2
    exit 2
  fi
  if [ "$ELEPHC_TARGET" != "ios-sim-arm64" ]; then
    echo "--run is Simulator-only: $ELEPHC_TARGET needs signing and a device to run on" >&2
    exit 2
  fi
  RUN_LIVE=1
fi

case "$ELEPHC_TARGET:$RUST_TARGET:$APPLE_SDK:$CLANG_TARGET" in
  ios-arm64:aarch64-apple-ios:iphoneos:arm64-apple-ios13.0) ;;
  ios-sim-arm64:aarch64-apple-ios-sim:iphonesimulator:arm64-apple-ios13.0-simulator) ;;
  *)
    echo "unsupported iOS curl link tuple: $ELEPHC_TARGET/$RUST_TARGET/$APPLE_SDK/$CLANG_TARGET" >&2
    exit 2
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures/ios-curl-link"
ELEPHC_BIN="$PROJECT_DIR/target/debug/elephc"
NATIVE_CACHE="${ELEPHC_NATIVE_CACHE:?ELEPHC_NATIVE_CACHE must name the isolated CI cache}"
SDK_PATH="$(xcrun --sdk "$APPLE_SDK" --show-sdk-path)"

# Wrapper paths are stable across CI runs so the managed-native toolchain fingerprint can
# reuse the actions/cache entry. Their contents bake in the SDK and target because recipe
# commands run with a scrubbed environment.
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

echo "==> building compiler and materializing curl for $ELEPHC_TARGET"
(cd "$PROJECT_DIR" && cargo build --bin elephc)
"$ELEPHC_BIN" native install --locked \
  --target "$ELEPHC_TARGET" \
  --manifest-path "$PROJECT_DIR/examples/curl-get/elephc.toml"

echo "==> cross-building the Rust curl bridge for $RUST_TARGET"
(cd "$PROJECT_DIR" && cargo build -p elephc-curl --target "$RUST_TARGET")
BRIDGE_ARCHIVE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_curl.a"
test -s "$BRIDGE_ARCHIVE"

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

CURL_ARCHIVE="$(find_native_archive curl 8.21.0 4 libcurl.a)"
LIBSSH2_ARCHIVE="$(find_native_archive libssh2 1.11.1 2 libssh2.a)"
LIBSSL_ARCHIVE="$(find_native_archive openssl 3.5.8 1 libssl.a)"
LIBCRYPTO_ARCHIVE="$(find_native_archive openssl 3.5.8 1 libcrypto.a)"
ZLIB_ARCHIVE="$(find_native_archive zlib 1.3.2 1 libz.a)"
NGHTTP2_ARCHIVE="$(find_native_archive nghttp2 1.70.0 2 libnghttp2.a)"

WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-curl-link.XXXXXX")"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# Compile-time evidence that this is the SecTrust build, not merely an iOS-shaped archive
# that would link while still having no default trust anchors at runtime.
strings "$CURL_ARCHIVE" > "$WORK_DIR/curl.strings"
grep -F "AppleSecTrust" "$WORK_DIR/curl.strings"
nm -u "$CURL_ARCHIVE" > "$WORK_DIR/curl.undefined"
grep -F "SecTrustCreateWithCertificates" "$WORK_DIR/curl.undefined"

cp "$FIXTURE_DIR/main.php" "$FIXTURE_DIR/host.c" "$WORK_DIR/"

# host.c includes <curl/curl.h> to drive the live transfer; the headers sit beside the
# archive the recipe installed.
CURL_INCLUDE_DIR="$(cd "$(dirname "$CURL_ARCHIVE")/../include" && pwd)"
test -f "$CURL_INCLUDE_DIR/curl/curl.h"
cp "$PROJECT_DIR/examples/curl-get/elephc.toml" "$WORK_DIR/elephc.toml"
cp "$PROJECT_DIR/examples/curl-get/elephc.lock" "$WORK_DIR/elephc.lock"

echo "==> compiling curl-using PHP as an $ELEPHC_TARGET staticlib"
"$ELEPHC_BIN" build --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
test -s "$WORK_DIR/libmain.a"
test -s "$WORK_DIR/libmain.h"

echo "==> linking the complete iOS host/native/bridge graph"
xcrun --sdk "$APPLE_SDK" clang \
  -target "$CLANG_TARGET" \
  -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" \
  -I "$CURL_INCLUDE_DIR" \
  "$WORK_DIR/host.c" \
  "$WORK_DIR/libmain.a" \
  "-Wl,-force_load,$BRIDGE_ARCHIVE" \
  "$CURL_ARCHIVE" \
  "$LIBSSH2_ARCHIVE" \
  "$LIBSSL_ARCHIVE" \
  "$LIBCRYPTO_ARCHIVE" \
  "$ZLIB_ARCHIVE" \
  "$NGHTTP2_ARCHIVE" \
  -framework Security \
  -framework CoreFoundation \
  -framework CoreServices \
  -framework SystemConfiguration \
  -o "$WORK_DIR/ios-curl-host"

test -s "$WORK_DIR/ios-curl-host"
xcrun vtool -show-build "$WORK_DIR/ios-curl-host"
echo "SecTrust curl compile/link succeeded for $ELEPHC_TARGET"

if [ "$RUN_LIVE" -eq 0 ]; then
  exit 0
fi

echo "==> running one live HTTPS GET inside the Simulator"
command -v jq >/dev/null || { echo "jq is required to select a Simulator device" >&2; exit 1; }

# Any available iOS device will do -- the transfer goes through the host's network stack.
# Restricting to the iOS runtimes keeps a watchOS or tvOS device from being picked, which
# cannot run an aarch64-apple-ios-sim binary.
DEVICE_ID="$(xcrun simctl list devices available --json \
  | jq -r '.devices
           | to_entries
           | map(select(.key | test("com\\.apple\\.CoreSimulator\\.SimRuntime\\.iOS")))
           | map(.value[])
           | map(select(.isAvailable))
           | (.[0].udid // empty)')"
if [ -z "$DEVICE_ID" ]; then
  echo "no available iOS Simulator device on this runner" >&2
  exit 1
fi
echo "Simulator device: $DEVICE_ID"

# Leave the device as it was found: a runner image ships these pre-created, so booting one
# is borrowing it rather than owning it.
WAS_BOOTED="$(xcrun simctl list devices --json | jq -r --arg id "$DEVICE_ID" \
  '[.devices[][] | select(.udid == $id) | .state] | (.[0] // "Unknown")')"
if [ "$WAS_BOOTED" != "Booted" ]; then
  xcrun simctl boot "$DEVICE_ID"
  shutdown_device() { xcrun simctl shutdown "$DEVICE_ID" >/dev/null 2>&1 || true; cleanup; }
  trap shutdown_device EXIT
  xcrun simctl bootstatus "$DEVICE_ID" -b
fi

xcrun simctl spawn "$DEVICE_ID" "$WORK_DIR/ios-curl-host" --live
echo "live SecTrust HTTPS transfer succeeded for $ELEPHC_TARGET"
