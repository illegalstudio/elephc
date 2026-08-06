#!/usr/bin/env bash
# Builds the elephc sandbox probe as an iOS app and runs it.
#
# Usage:
#   ./run.sh simulator      # booted simulator (no signing needed)
#   ./run.sh device         # physical device (needs a signing identity)
#   ./run.sh device --keep  # keep the built bundle
#
# Override the bundle identifier when your provisioning profile is not a
# wildcard:  ELEPHC_PROBE_BUNDLE_ID=com.example.probe ./run.sh device
#
# Why this exists: the iOS Simulator runs on the *macOS* kernel, so a simulator
# run exercises the same syscall table elephc was written against and proves
# nothing about a device sandbox. elephc emits 225 raw syscalls; the ~26
# path-based and network ones are the open question. This probe answers it by
# measurement rather than argument -- run it in both modes and diff the reports.
#
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$HERE/../.." && pwd)"
BUNDLE_ID="${ELEPHC_PROBE_BUNDLE_ID:-dev.elephc.probe}"

MODE="${1:-device}"
KEEP=0
for arg in "$@"; do [ "$arg" = "--keep" ] && KEEP=1; done

case "$MODE" in
  simulator|sim)
    SDK=iphonesimulator; ELEPHC_TARGET=ios-sim-arm64
    RUST_TARGET=aarch64-apple-ios-sim; SWIFT_TRIPLE=arm64-apple-ios15.0-simulator
    PLATFORM_NAME=iPhoneSimulator ;;
  device)
    SDK=iphoneos; ELEPHC_TARGET=ios-arm64
    RUST_TARGET=aarch64-apple-ios; SWIFT_TRIPLE=arm64-apple-ios15.0
    PLATFORM_NAME=iPhoneOS ;;
  *) echo "usage: $0 [simulator|device] [--keep]" >&2; exit 2 ;;
esac

APP="$HERE/ElephcProbe-$MODE.app"

if ! SDK_PATH="$(xcrun --sdk "$SDK" --show-sdk-path 2>/dev/null)" || [ -z "$SDK_PATH" ]; then
  echo "No '$SDK' SDK. Install full Xcode and accept its licence." >&2
  exit 1
fi

# --- build the three pieces -------------------------------------------------

ELEPHC="${ELEPHC_BIN:-$PROJECT_DIR/target/debug/elephc}"
[ -x "$ELEPHC" ] || (cd "$PROJECT_DIR" && cargo build)

echo "==> compiling probe.php for $ELEPHC_TARGET"
(cd "$HERE" && XDG_CACHE_HOME="$HERE/.cache" "$ELEPHC" --target "$ELEPHC_TARGET" --emit staticlib probe.php)

# Any PHP touching the filesystem reaches __rt_fopen_maybe_phar, so the phar
# bridge is not optional here. Bridges are ordinary Rust staticlibs and must be
# cross-compiled for the same target; `Emit::Staticlib` deliberately leaves them
# to the consumer rather than rolling them into the archive.
echo "==> building the phar bridge for $RUST_TARGET"
rustup target list --installed | grep -qx "$RUST_TARGET" || rustup target add "$RUST_TARGET"
(cd "$PROJECT_DIR" && cargo build -p elephc-phar --target "$RUST_TARGET" 2>&1 | tail -1)
BRIDGE="$PROJECT_DIR/target/$RUST_TARGET/debug/libelephc_phar.a"

echo "==> compiling the SwiftUI host"
# -sdk covers compilation but not the link, where swiftc drives clang with the
# host sysroot unless -isysroot is passed through explicitly.
swiftc -O -parse-as-library \
       -target "$SWIFT_TRIPLE" -sdk "$SDK_PATH" \
       -Xclang-linker -isysroot -Xclang-linker "$SDK_PATH" \
       -import-objc-header "$HERE/probe_abi.h" \
       -o "$HERE/ElephcProbe" \
       "$HERE/ProbeApp.swift" "$HERE/libprobe.a" "$BRIDGE" \
       -lbz2 -lz

# --- assemble the bundle ----------------------------------------------------

echo "==> assembling $APP"
rm -rf "$APP"; mkdir -p "$APP"
mv "$HERE/ElephcProbe" "$APP/ElephcProbe"

cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>               <string>elephc probe</string>
    <key>CFBundleDisplayName</key>        <string>elephc probe</string>
    <key>CFBundleIdentifier</key>         <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>         <string>ElephcProbe</string>
    <key>CFBundlePackageType</key>        <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>1.0</string>
    <key>CFBundleVersion</key>            <string>1</string>
    <key>MinimumOSVersion</key>           <string>15.0</string>
    <key>UILaunchScreen</key>             <dict/>
    <key>CFBundleSupportedPlatforms</key> <array><string>$PLATFORM_NAME</string></array>
    <key>UIDeviceFamily</key>             <array><integer>1</integer><integer>2</integer></array>
</dict>
</plist>
PLIST

# --- simulator: ad-hoc signature is enough ----------------------------------

if [ "$MODE" != "device" ]; then
  codesign --force --sign - "$APP" >/dev/null 2>&1 || true
  DEVICE="$(xcrun simctl list devices booted -j 2>/dev/null \
            | grep -o '"udid" : "[^"]*"' | head -1 | cut -d'"' -f4 || true)"
  if [ -z "$DEVICE" ]; then
    echo "Built, but no simulator is booted. Try: xcrun simctl boot \"iPhone 17 Pro\"" >&2
    exit 0
  fi
  echo "==> running in simulator $DEVICE"
  xcrun simctl spawn "$DEVICE" "$APP/ElephcProbe" --stdout
  [ "$KEEP" = "1" ] || rm -rf "$APP"
  exit 0
fi

# --- device: real signature, embedded profile, devicectl --------------------

IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
            | grep -o '"Apple Develop[^"]*"' | head -1 | tr -d '"' || true)"
PROFILE="$(ls -t ~/Library/MobileDevice/Provisioning\ Profiles/*.mobileprovision 2>/dev/null | head -1 || true)"

if [ -z "$IDENTITY" ] || [ -z "$PROFILE" ]; then
  cat >&2 <<EOF

Cannot sign for a device yet.

  signing identity : ${IDENTITY:-none found}
  provisioning     : ${PROFILE:-none found}

The reliable way to create both is to let Xcode provision the device once:

  1. Open Xcode ▸ Settings ▸ Accounts and add your Apple ID (a free one works).
  2. Connect and trust the iPhone.
  3. Create any empty iOS App project, set its team, and Run it on the device.
     That issues an "Apple Development" certificate and a provisioning profile.
  4. Re-run this script. If the profile is not a wildcard, pass a matching id:
       ELEPHC_PROBE_BUNDLE_ID=<the project's bundle id> $0 device

Everything else is already built: $APP is assembled and only needs a signature.
EOF
  exit 1
fi

echo "==> embedding $(basename "$PROFILE")"
cp "$PROFILE" "$APP/embedded.mobileprovision"

# The entitlements have to come from the profile itself: signing with anything
# the profile does not grant is rejected at install time, not at sign time.
ENT="$HERE/.entitlements.plist"
security cms -D -i "$PROFILE" > "$HERE/.profile.plist" 2>/dev/null
/usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "$HERE/.profile.plist" > "$ENT"

echo "==> signing as $IDENTITY"
codesign --force --sign "$IDENTITY" --entitlements "$ENT" --timestamp=none "$APP"
codesign --verify --verbose=2 "$APP" 2>&1 | sed 's/^/    /'
rm -f "$HERE/.profile.plist" "$ENT"

UDID="$(xcrun devicectl list devices 2>/dev/null | awk '/connected/ {print $(NF-1); exit}')"
if [ -z "$UDID" ]; then
  echo "No connected device found. Plug in an iPhone, unlock it and trust this Mac." >&2
  echo "The signed bundle is ready at: $APP" >&2
  exit 1
fi

echo "==> installing on $UDID"
xcrun devicectl device install app --device "$UDID" "$APP" 2>&1 | tail -3

echo "==> launching with console output"
xcrun devicectl device process launch --device "$UDID" --console "$BUNDLE_ID" 2>&1 | tail -40

[ "$KEEP" = "1" ] || rm -rf "$APP"
