#!/usr/bin/env bash
# Builds the SwiftUI host as a real iOS app, installs it on a booted simulator,
# launches it, and captures a screenshot.
#
# Usage:
#   ./run-ios.sh                 # build, install, launch, screenshot
#   ./run-ios.sh --selftest      # run the headless round-trip check instead
#
# The interface is decided entirely by compiled PHP: `view.php` is compiled to
# an arm64 iOS static library and linked into the app, which asks it for a view
# tree and renders it as SwiftUI. No .xcodeproj is involved — swiftc and the
# simulator SDK are enough.
#
# Requires full Xcode with its licence accepted, an installed iOS runtime, and a
# booted simulator:
#
#   xcodebuild -downloadPlatform iOS
#   xcrun simctl boot "iPhone 17 Pro"
#
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$HERE/../.." && pwd)"
APP="$HERE/ViewProtocolIOS.app"
BUNDLE_ID="dev.elephc.example.viewprotocol"
SELFTEST=0
[ "${1:-}" = "--selftest" ] && SELFTEST=1

if ! SDK_PATH="$(xcrun --sdk iphonesimulator --show-sdk-path 2>/dev/null)" || [ -z "$SDK_PATH" ]; then
  echo "No iphonesimulator SDK. Install full Xcode and accept its licence." >&2
  exit 1
fi

DEVICE="$(xcrun simctl list devices booted -j 2>/dev/null \
          | grep -o '"udid" : "[^"]*"' | head -1 | cut -d'"' -f4 || true)"
if [ -z "$DEVICE" ] && [ "$SELFTEST" = "0" ]; then
  echo "No simulator is booted. Try: xcrun simctl boot \"iPhone 17 Pro\"" >&2
  exit 1
fi

ELEPHC="${ELEPHC_BIN:-$PROJECT_DIR/target/debug/elephc}"
if [ ! -x "$ELEPHC" ]; then
  echo "==> building elephc"
  (cd "$PROJECT_DIR" && cargo build)
fi

echo "==> compiling view.php for ios-sim-arm64"
(cd "$HERE" && "$ELEPHC" --target ios-sim-arm64 --emit staticlib view.php)

echo "==> archive members and their Mach-O platform"
(cd "$HERE" && for member in $(ar t libview.a | grep -v SYMDEF); do
  ar x libview.a "$member"
  printf '    %-13s %s\n' "$(vtool -show-build "$member" | grep -Eo 'IOS[A-Z]*|MACOS' | head -1)" "$member"
  rm -f "$member"
done)

echo "==> compiling the SwiftUI host for the simulator"
# -import-objc-header brings in the C declarations of ElephcStr and the exports;
# without it the @convention(c) types are rejected as not C-representable.
# -parse-as-library is required by @main.
# -sdk covers compilation, but swiftc drives clang for the link step and that
# driver defaults to the host sysroot -- hence the explicit -isysroot, without
# which it warns "using sysroot for 'MacOSX' but targeting 'iPhone'".
swiftc -O -parse-as-library \
       -target arm64-apple-ios15.0-simulator -sdk "$SDK_PATH" \
       -Xclang-linker -isysroot -Xclang-linker "$SDK_PATH" \
       -import-objc-header "$HERE/elephc_abi.h" \
       -o "$HERE/ViewProtocolIOS" \
       "$HERE/ViewProtocolApp.swift" "$HERE/libview.a"

if [ "$SELFTEST" = "1" ]; then
  if [ -z "$DEVICE" ]; then
    echo "No simulator booted; cannot run the self-test." >&2
    exit 1
  fi
  echo "==> self-test inside simulator $DEVICE"
  xcrun simctl spawn "$DEVICE" "$HERE/ViewProtocolIOS" --selftest
  rm -f "$HERE/ViewProtocolIOS"
  exit 0
fi

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP"
mv "$HERE/ViewProtocolIOS" "$APP/ViewProtocolIOS"

# An iOS bundle is flat: the executable sits at the bundle root, not under
# Contents/MacOS the way a macOS bundle does.
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                  <string>elephc</string>
    <key>CFBundleDisplayName</key>           <string>elephc → SwiftUI</string>
    <key>CFBundleIdentifier</key>            <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>            <string>ViewProtocolIOS</string>
    <key>CFBundlePackageType</key>           <string>APPL</string>
    <key>CFBundleShortVersionString</key>    <string>1.0</string>
    <key>CFBundleVersion</key>               <string>1</string>
    <key>MinimumOSVersion</key>              <string>15.0</string>
    <key>UILaunchScreen</key>                <dict/>
    <key>CFBundleSupportedPlatforms</key>    <array><string>iPhoneSimulator</string></array>
    <key>UIDeviceFamily</key>                <array><integer>1</integer><integer>2</integer></array>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "==> installing on simulator $DEVICE"
xcrun simctl install "$DEVICE" "$APP"

echo "==> launching"
xcrun simctl launch "$DEVICE" "$BUNDLE_ID" | sed 's/^/    /'

# The first frame needs a moment; the screenshot is the point of this script.
python3 -c "import time; time.sleep(4)"

SHOT="$HERE/simulator.png"
xcrun simctl io "$DEVICE" screenshot "$SHOT" >/dev/null 2>&1
echo "==> screenshot: $SHOT"

cat <<EOF

The counter, its pluralised label and every button are computed by compiled PHP.
Tap +/- in the simulator and watch state that lives in a PHP function static.
EOF
