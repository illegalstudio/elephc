#!/usr/bin/env bash
# Cross-builds the URI bridge and links a real iOS host on both Apple ABIs.
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
  *) echo "unsupported uri iOS tuple" >&2; exit 2 ;;
esac
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
SDK_PATH="$(xcrun --sdk "$APPLE_SDK" --show-sdk-path)"
WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-uri-link.XXXXXX")"
trap 'rm -rf "$WORK_DIR"' EXIT

(cd "$PROJECT_DIR" && cargo build --locked --bin elephc)
(cd "$PROJECT_DIR" && cargo build --locked -p elephc-uri --target "$RUST_TARGET")
cp "$SCRIPT_DIR/fixtures/ios-uri-link/main.php" "$SCRIPT_DIR/fixtures/ios-uri-link/host.c" "$WORK_DIR/"
"$PROJECT_DIR/target/debug/elephc" --with-uri --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
nm -u "$WORK_DIR/libmain.a" > "$WORK_DIR/undefined"
grep -F elephc_uri_parse "$WORK_DIR/undefined"
grep -F elephc_uri_component "$WORK_DIR/undefined"
xcrun --sdk "$APPLE_SDK" clang -target "$CLANG_TARGET" -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" "$WORK_DIR/host.c" "$WORK_DIR/libmain.a" \
  "$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_uri.a" \
  -o "$WORK_DIR/ios-uri-host"
test -s "$WORK_DIR/ios-uri-host"
xcrun vtool -show-build "$WORK_DIR/ios-uri-host"
