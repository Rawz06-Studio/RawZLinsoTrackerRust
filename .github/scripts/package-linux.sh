#!/usr/bin/env bash
# Wraps the client binary in an AppImage, a single runnable file with an icon.
# Usage: package-linux.sh <binary> <output.AppImage>
set -euo pipefail

binary=$1
output=$2

tool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage

work=$(mktemp -d)
appdir="$work/RawZLinsoTracker.AppDir"
mkdir -p "$appdir/usr/bin"
install -m 755 "$binary" "$appdir/usr/bin/rawzlinsotracker"
ln -s usr/bin/rawzlinsotracker "$appdir/AppRun"
cp public/default/icon.png "$appdir/rawzlinsotracker.png"

cat >"$appdir/rawzlinsotracker.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=RawZLinsoTracker
Exec=rawzlinsotracker
Icon=rawzlinsotracker
Categories=Game;
EOF

curl -fsSL -o "$work/appimagetool" "$tool_url"
chmod +x "$work/appimagetool"

# CI machines have no FUSE to mount the tool's own AppImage with.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$work/appimagetool" --no-appstream "$appdir" "$output"
