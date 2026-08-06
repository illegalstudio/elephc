#!/usr/bin/env bash
# Builds a static library of compiled PHP for iOS, links it into a host binary,
# and — on the simulator, when one is booted — runs it.
#
# Usage:
#   ./scripts/ios-relink-spike.sh                 # iOS Simulator (default)
#   ./scripts/ios-relink-spike.sh device          # iOS device (build + link only)
#   ./scripts/ios-relink-spike.sh --keep          # keep the work directory
#
# This exercises the shipping path: `elephc --target ios-* --emit staticlib`.
# An earlier version assembled for macOS and relinked against the iOS SDK by
# hand, on the theory that only the SDK differed. It does not — a Mach-O object
# records the platform it was assembled for, and ld refuses to mix them:
#
#   ld: building for 'iOS-simulator', but linking in object file built for 'macOS'
#
# The compiler now assembles through clang with the right -target, so there is
# nothing left for this script to work around.
#
# Requires full Xcode. The Command Line Tools alone carry no iOS SDK; if that is
# all that is installed, elephc says so and exits without doing damage.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE="simulator"
KEEP=0
for arg in "$@"; do
  case "$arg" in
    simulator|sim) MODE="simulator" ;;
    device)        MODE="device" ;;
    --keep)        KEEP=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

if [ "$MODE" = "simulator" ]; then
  SDK="iphonesimulator"
  ELEPHC_TARGET="ios-sim-arm64"
  HOST_TRIPLE="arm64-apple-ios13.0-simulator"
else
  SDK="iphoneos"
  ELEPHC_TARGET="ios-arm64"
  HOST_TRIPLE="arm64-apple-ios13.0"
fi

if ! SDK_PATH="$(xcrun --sdk "$SDK" --show-sdk-path 2>/dev/null)" || [ -z "$SDK_PATH" ]; then
  cat >&2 <<EOF
No '$SDK' SDK found.

xcode-select currently points at:
  $(xcode-select -p 2>/dev/null || echo '<unset>')

The Command Line Tools do not ship iOS SDKs. Install full Xcode, point at it,
and accept the licence:

  sudo xcode-select -s /Applications/Xcode.app
  sudo xcodebuild -license accept
EOF
  exit 1
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/elephc_ios_spike.XXXXXX")"
cleanup() { [ "$KEEP" = "1" ] || rm -rf "$WORK"; }
trap cleanup EXIT
[ "$KEEP" = "1" ] && echo "work directory: $WORK"

ELEPHC="${ELEPHC_BIN:-$PROJECT_DIR/target/debug/elephc}"
if [ ! -x "$ELEPHC" ]; then
  echo "building elephc ..." >&2
  (cd "$PROJECT_DIR" && cargo build)
fi

cat > "$WORK/spike.php" <<'PHP'
<?php
#[Export]
function spike_add(int $a, int $b): int {
    return $a + $b;
}

#[Export]
function spike_greet(string $name): string {
    return "hi " . $name;
}
PHP

# Isolated so the runtime object cache holds exactly this build's artifact.
export XDG_CACHE_HOME="$WORK/cache"

echo "==> compiling for $ELEPHC_TARGET"
(cd "$WORK" && "$ELEPHC" --target "$ELEPHC_TARGET" --emit staticlib spike.php)

echo "==> archive members and their Mach-O platform"
(cd "$WORK" && for member in $(ar t libspike.a | grep -v SYMDEF); do
  ar x libspike.a "$member"
  printf '    %-8s %s\n' "$(vtool -show-build "$member" | grep -Eo 'IOS[A-Z]*|MACOS' | head -1)" "$member"
  rm -f "$member"
done)

cat > "$WORK/host.c" <<'C'
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>

typedef struct { const char *ptr; size_t len; } elephc_str;

extern int32_t elephc_init(void);
extern void elephc_free(void *);
extern int64_t spike_add(int64_t, int64_t);
extern elephc_str spike_greet(const char *, size_t);

int main(void) {
    if (elephc_init() != 0) return 1;
    elephc_str g = spike_greet("iOS", 3);
    printf("%lld %.*s %zu\n", (long long)spike_add(40, 2), (int)g.len, g.ptr, g.len);
    elephc_free((void *)g.ptr);
    return 0;
}
C

echo "==> linking a $HOST_TRIPLE host against the archive"
xcrun --sdk "$SDK" clang -target "$HOST_TRIPLE" -isysroot "$SDK_PATH" \
      -o "$WORK/host" "$WORK/host.c" "$WORK/libspike.a"
vtool -show-build "$WORK/host" | sed 's/^/    /'

if [ "$MODE" = "device" ]; then
  cat <<EOF

Linked for a device. Running it needs provisioning and a signed app bundle, so
the spike stops here: the link succeeding is the answer it was asked for.
EOF
  exit 0
fi

DEVICE="$(xcrun simctl list devices booted -j 2>/dev/null \
          | grep -o '"udid" : "[^"]*"' | head -1 | cut -d'"' -f4 || true)"
if [ -z "$DEVICE" ]; then
  cat <<EOF

Built and linked, but no simulator is booted, so nothing was executed. Boot one
and re-run for the end-to-end answer:

  xcrun simctl list devices            # if empty, no runtime is installed
  xcodebuild -downloadPlatform iOS     # installs one (several GB)
  xcrun simctl boot "iPhone 16"
EOF
  exit 0
fi

echo "==> running inside booted simulator $DEVICE"
OUTPUT="$(xcrun simctl spawn "$DEVICE" "$WORK/host")"
echo "    $OUTPUT"

EXPECTED="42 hi iOS 6"
if [ "$OUTPUT" != "$EXPECTED" ]; then
  echo "unexpected output: got '$OUTPUT', want '$EXPECTED'" >&2
  exit 1
fi

cat <<EOF

Lot 0 answered: compiled PHP builds for iOS, links into a native host through
the same C ABI the cdylib path exposes, and runs on the simulator. Both export
return shapes work and the string result was released through elephc_free.
EOF
