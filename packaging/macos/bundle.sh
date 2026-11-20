#!/bin/sh
# Builds TuneCraft in release mode and assembles dist/TuneCraft.app (unsigned).
# Usage: sh packaging/macos/bundle.sh        (run from the app folder)
set -eu

cd "$(dirname "$0")/../.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

cargo build --release

APP=dist/TuneCraft.app
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/tunecraft "$APP/Contents/MacOS/TuneCraft"
cp assets/app-icon/tunecraft.icns "$APP/Contents/Resources/TuneCraft.icns"
cp assets/fonts/OFL-Inter.txt assets/fonts/OFL-JetBrainsMono.txt "$APP/Contents/Resources/"
cp ATTRIBUTION.md NOTICE LICENSE-MIT LICENSE-APACHE "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>TuneCraft</string>
  <key>CFBundleDisplayName</key><string>TuneCraft</string>
  <key>CFBundleIdentifier</key><string>ai.storyteller.tunecraft</string>
  <key>CFBundleExecutable</key><string>TuneCraft</string>
  <key>CFBundleIconFile</key><string>TuneCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSAppTransportSecurity</key>
  <dict><key>NSAllowsArbitraryLoads</key><true/></dict>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Audio file</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>mp3</string><string>m4a</string><string>aac</string><string>flac</string>
        <string>wav</string><string>aif</string><string>aiff</string><string>ogg</string>
        <string>oga</string><string>mka</string><string>caf</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST

# ad-hoc signature so macOS will open it locally (not a Developer ID signature)
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "Built $APP (version $VERSION)"
