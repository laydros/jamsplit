#!/usr/bin/env bash
# Stage and archive one release bundle. Inputs: release binaries in
# target/release/, sidecars from scripts/fetch-sidecars.sh. Output lands
# in dist/.
set -euo pipefail

target="${1:?usage: package.sh <macos-arm64|linux-x86_64|windows-x86_64>}"
version="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
name="jamsplit-$version-$target"
stage="dist/$name"
side="sidecars/$target"

rm -rf "$stage"
mkdir -p "$stage"
cp LICENSE packaging/NOTICE.txt packaging/README.txt "$stage/"

case "$target" in
  macos-arm64)
    app="$stage/jamsplit.app/Contents"
    mkdir -p "$app/MacOS" "$app/Resources"
    cp target/release/jamsplit-gui "$app/MacOS/"
    cp "$side/ffmpeg" "$side/ffprobe" "$app/MacOS/"
    cp assets/icons/jamsplit.icns "$app/Resources/"
    cat > "$app/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>jamsplit</string>
  <key>CFBundleDisplayName</key><string>jamsplit</string>
  <key>CFBundleIdentifier</key><string>net.laydros.jamsplit</string>
  <key>CFBundleExecutable</key><string>jamsplit-gui</string>
  <key>CFBundleIconFile</key><string>jamsplit.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
    cp target/release/jamsplit "$stage/"
    # Ad-hoc sign innermost-first; modifying bundle contents after signing
    # would invalidate the seal.
    codesign --force -s - "$app/MacOS/ffmpeg" "$app/MacOS/ffprobe"
    codesign --force -s - "$stage/jamsplit"
    codesign --force -s - "$stage/jamsplit.app"
    (cd dist && ditto -c -k --keepParent "$name" "$name.zip")
    ;;
  linux-x86_64)
    cp target/release/jamsplit target/release/jamsplit-gui "$stage/"
    cp "$side/ffmpeg" "$side/ffprobe" "$stage/"
    chmod 755 "$stage/jamsplit" "$stage/jamsplit-gui" "$stage/ffmpeg" "$stage/ffprobe"
    tar -czf "dist/$name.tar.gz" -C dist "$name"
    ;;
  windows-x86_64)
    cp target/release/jamsplit.exe target/release/jamsplit-gui.exe "$stage/"
    cp "$side/ffmpeg.exe" "$side/ffprobe.exe" "$stage/"
    (cd dist && 7z a -tzip "$name.zip" "$name" >/dev/null)
    ;;
  *)
    echo "unknown target: $target" >&2; exit 1 ;;
esac

rm -rf "$stage"
ls -lh dist/
