#!/usr/bin/env bash
# Wraps the client binary in an ad-hoc signed .app, inside a .dmg.
# Usage: package-macos.sh <binary> <version> <output.dmg>
set -euo pipefail

binary=$1
version=$2
output=$3

work=$(mktemp -d)
app="$work/dmg/RawZLinsoTracker.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/RawZLinsoTracker"

iconset="$work/icon.iconset"
mkdir "$iconset"
resize() {
  sips -z "$1" "$1" public/default/icon.png --out "$iconset/$2" >/dev/null
}
for size in 16 32 128 256; do
  resize "$size" "icon_${size}x${size}.png"
  resize "$((size * 2))" "icon_${size}x${size}@2x.png"
done
iconutil --convert icns --output "$app/Contents/Resources/icon.icns" "$iconset"

cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>RawZLinsoTracker</string>
  <key>CFBundleExecutable</key>
  <string>RawZLinsoTracker</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>CFBundleIdentifier</key>
  <string>fr.rawz06.tracker</string>
  <key>CFBundleName</key>
  <string>RawZLinsoTracker</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

codesign --force --sign - "$app"
codesign --verify --strict "$app"

ln -s /Applications "$work/dmg/Applications"
hdiutil create -quiet -ov -volname RawZLinsoTracker -srcfolder "$work/dmg" -format UDZO "$output"
