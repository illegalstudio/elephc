#!/usr/bin/env bash
# Cross-builds the PHP tokenizer bridge and links a real iOS host on both Apple ABIs.
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
  *) echo "unsupported tokenizer iOS tuple" >&2; exit 2 ;;
esac
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
SDK_PATH="$(xcrun --sdk "$APPLE_SDK" --show-sdk-path)"
WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-tokenizer-link.XXXXXX")"
trap 'rm -rf "$WORK_DIR"' EXIT

(cd "$PROJECT_DIR" && cargo build --locked --bin elephc)
(cd "$PROJECT_DIR" && cargo build --locked -p elephc-tokenizer --target "$RUST_TARGET")
cp "$SCRIPT_DIR/fixtures/ios-tokenizer-link/main.php" "$SCRIPT_DIR/fixtures/ios-tokenizer-link/host.c" "$WORK_DIR/"
"$PROJECT_DIR/target/debug/elephc" --with-tokenizer --target "$ELEPHC_TARGET" --emit staticlib "$WORK_DIR/main.php"
nm -u "$WORK_DIR/libmain.a" > "$WORK_DIR/undefined"
grep -F elephc_tokenizer_scan "$WORK_DIR/undefined"
grep -F elephc_tokenizer_name "$WORK_DIR/undefined"
xcrun --sdk "$APPLE_SDK" clang -target "$CLANG_TARGET" -isysroot "$SDK_PATH" \
  -I "$WORK_DIR" "$WORK_DIR/host.c" "$WORK_DIR/libmain.a" \
  "$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_tokenizer.a" \
  -o "$WORK_DIR/ios-tokenizer-host"
test -s "$WORK_DIR/ios-tokenizer-host"
xcrun vtool -show-build "$WORK_DIR/ios-tokenizer-host"
