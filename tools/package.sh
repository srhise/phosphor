#!/usr/bin/env bash
# Build Phosphor.app. macOS application bundles are just a directory with a
# plist, so we make one directly rather than depend on a packaging tool.
set -euo pipefail
cd "$(dirname "$0")/.."

# Make sure the Rust toolchain is on PATH when run from a GUI or hook.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
APP="target/Phosphor.app"

# release.sh points this at a universal binary; by default build native.
BIN=${PHOSPHOR_BIN:-target/release/phosphor}
if [ "$BIN" = "target/release/phosphor" ]; then
    echo "==> building release binary"
    cargo build --release
fi

echo "==> rendering icon"
python3 tools/make-icon.py target/icon.bmp
ICONSET=target/phosphor.iconset
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
sips -s format png target/icon.bmp --out target/icon.png >/dev/null
for size in 16 32 64 128 256 512 1024; do
    sips -z $size $size target/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
done
# Retina variants are the next size up under an @2x name.
for size in 16 32 128 256 512; do
    cp "$ICONSET/icon_$((size*2))x$((size*2)).png" "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o target/phosphor.icns

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/phosphor"
cp target/phosphor.icns "$APP/Contents/Resources/phosphor.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>Phosphor</string>
    <key>CFBundleDisplayName</key>       <string>phosphor</string>
    <key>CFBundleIdentifier</key>        <string>com.craftedup.phosphor</string>
    <key>CFBundleVersion</key>           <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key>        <string>phosphor</string>
    <key>CFBundleIconFile</key>          <string>phosphor</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>LSMinimumSystemVersion</key>    <string>11.0</string>
    <key>NSHighResolutionCapable</key>   <true/>
</dict>
</plist>
PLIST

# Gatekeeper only accepts apps signed with a Developer ID certificate and
# the hardened runtime. Use one when present (or given via SIGN_IDENTITY);
# ad-hoc is fine for a build that never leaves this machine.
IDENTITY=${SIGN_IDENTITY:-$(security find-identity -v -p codesigning \
    | awk -F'"' '/Developer ID Application/ {print $2; exit}')}
if [ -n "$IDENTITY" ]; then
    echo "==> signing ($IDENTITY)"
    codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
else
    echo "==> signing (ad-hoc, for local use)"
    codesign --force --sign - "$APP"
fi

echo "built $APP"
