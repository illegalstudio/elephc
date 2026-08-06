#!/usr/bin/env bash
# Builds and launches the SwiftUI host driven by compiled PHP.
#
# Usage:
#   ./run.sh                # build and launch
#   ./run.sh --build-only   # build the .app bundle without launching
#
# Needs only the Xcode Command Line Tools: swiftc ships with them and SwiftUI is
# a system framework, so no Xcode install and no .xcodeproj are involved.
#
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$HERE/../.." && pwd)"
APP="$HERE/ViewProtocol.app"
LAUNCH=1
[ "${1:-}" = "--build-only" ] && LAUNCH=0

ELEPHC="${ELEPHC_BIN:-$PROJECT_DIR/target/debug/elephc}"
if [ ! -x "$ELEPHC" ]; then
  echo "==> building elephc"
  (cd "$PROJECT_DIR" && cargo build)
fi

echo "==> compiling view.php to a native static library"
(cd "$HERE" && "$ELEPHC" --emit staticlib view.php)

echo "==> compiling the SwiftUI host"
# -import-objc-header brings in the C declaration of ElephcStr, without which
# the @convention(c) signatures are rejected as not C-representable.
# -parse-as-library is required by @main: it tells swiftc the file defines a
# module rather than a script with top-level code.
# The library is linked statically, the same delivery form an Xcode project
# consumes, so the exports are ordinary C symbols rather than dlsym lookups.
swiftc -O -parse-as-library \
       -import-objc-header "$HERE/elephc_abi.h" \
       -o "$HERE/ViewProtocol" "$HERE/ViewProtocolApp.swift" "$HERE/libview.a"

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mv "$HERE/ViewProtocol" "$APP/Contents/MacOS/ViewProtocol"

# A SwiftUI app launched from a bundle gets a real activation policy, a Dock
# entry and a focusable window; the bare executable does not.
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>ViewProtocol</string>
    <key>CFBundleDisplayName</key>     <string>elephc → SwiftUI</string>
    <key>CFBundleIdentifier</key>      <string>dev.elephc.example.viewprotocol</string>
    <key>CFBundleExecutable</key>      <string>ViewProtocol</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>1.0</string>
    <key>LSMinimumSystemVersion</key>  <string>13.0</string>
    <key>NSHighResolutionCapable</key> <true/>
</dict>
</plist>
PLIST

# Ad-hoc signature: unsigned bundles are refused on Apple Silicon.
codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "==> built $APP"
if [ "$LAUNCH" = "1" ]; then
  open "$APP"
  echo "    launched — the counter and its labels are computed in compiled PHP"
else
  echo "    open it with: open '$APP'"
fi
