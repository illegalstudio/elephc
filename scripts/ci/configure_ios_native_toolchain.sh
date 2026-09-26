#!/usr/bin/env bash
# Configures the explicit Apple C toolchain required by managed native iOS builds.
# Source this file with one supported Elephc target so its exports remain in the caller.

if [ "$#" -ne 1 ]; then
  echo "usage: source scripts/ci/configure_ios_native_toolchain.sh <elephc-target>" >&2
  return 2
fi

ELEPHC_IOS_TARGET="$1"
case "$ELEPHC_IOS_TARGET" in
  ios-arm64)
    ELEPHC_IOS_SDK="iphoneos"
    ELEPHC_IOS_CLANG_TARGET="arm64-apple-ios13.0"
    ;;
  ios-sim-arm64)
    ELEPHC_IOS_SDK="iphonesimulator"
    ELEPHC_IOS_CLANG_TARGET="arm64-apple-ios13.0-simulator"
    ;;
  *)
    echo "unsupported iOS native target: $ELEPHC_IOS_TARGET" >&2
    return 2
    ;;
esac

ELEPHC_IOS_SDK_PATH="$(xcrun --sdk "$ELEPHC_IOS_SDK" --show-sdk-path)"
ELEPHC_IOS_TOOLCHAIN_DIR="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-ios-toolchain/$ELEPHC_IOS_TARGET"
mkdir -p "$ELEPHC_IOS_TOOLCHAIN_DIR"
ELEPHC_IOS_CC_WRAPPER="$ELEPHC_IOS_TOOLCHAIN_DIR/cc"
ELEPHC_IOS_AR_WRAPPER="$ELEPHC_IOS_TOOLCHAIN_DIR/ar"
ELEPHC_IOS_RANLIB_WRAPPER="$ELEPHC_IOS_TOOLCHAIN_DIR/ranlib"
ELEPHC_IOS_XCRUN_BIN="$(command -v xcrun)"
ELEPHC_IOS_AR_BIN="$(xcrun --sdk "$ELEPHC_IOS_SDK" --find ar)"
ELEPHC_IOS_RANLIB_BIN="$(xcrun --sdk "$ELEPHC_IOS_SDK" --find ranlib)"

printf '#!/usr/bin/env bash\nexec %q --sdk %q clang -target %q -isysroot %q "$@"\n' \
  "$ELEPHC_IOS_XCRUN_BIN" "$ELEPHC_IOS_SDK" "$ELEPHC_IOS_CLANG_TARGET" \
  "$ELEPHC_IOS_SDK_PATH" > "$ELEPHC_IOS_CC_WRAPPER"
printf '#!/usr/bin/env bash\nexec %q "$@"\n' "$ELEPHC_IOS_AR_BIN" > "$ELEPHC_IOS_AR_WRAPPER"
printf '#!/usr/bin/env bash\nexec %q "$@"\n' "$ELEPHC_IOS_RANLIB_BIN" > "$ELEPHC_IOS_RANLIB_WRAPPER"
chmod +x "$ELEPHC_IOS_CC_WRAPPER" "$ELEPHC_IOS_AR_WRAPPER" "$ELEPHC_IOS_RANLIB_WRAPPER"

ELEPHC_IOS_TARGET_ENV="$(printf '%s' "$ELEPHC_IOS_TARGET" | tr '[:lower:]-' '[:upper:]_')"
export "ELEPHC_NATIVE_CC_${ELEPHC_IOS_TARGET_ENV}=$ELEPHC_IOS_CC_WRAPPER"
export "ELEPHC_NATIVE_AR_${ELEPHC_IOS_TARGET_ENV}=$ELEPHC_IOS_AR_WRAPPER"
export "ELEPHC_NATIVE_RANLIB_${ELEPHC_IOS_TARGET_ENV}=$ELEPHC_IOS_RANLIB_WRAPPER"
