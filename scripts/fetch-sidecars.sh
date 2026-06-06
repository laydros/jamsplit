#!/usr/bin/env bash
# Download and verify the static ffmpeg/ffprobe sidecars bundled into release
# zips. Pins (URL + sha256) are the single source of truth for what we ship;
# bump procedure is in RELEASING.md.
set -euo pipefail

target="${1:?usage: fetch-sidecars.sh <macos-arm64|linux-x86_64|windows-x86_64>}"
dest="sidecars/$target"
# Clean slate per run: stale files from a previous extraction (or an older
# pin's different archive layout) must never leak into a bundle.
rm -rf "$dest"
mkdir -p "$dest"

# --- pins (resolved 2026-06-06; see RELEASING.md to bump) ---------------
BTBN_WIN_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-06-06-13-18/ffmpeg-n7.1.4-9-gc06af95f12-win64-gpl-7.1.zip"
BTBN_WIN_SHA="7570ea98a3bc04f45c14be7cc7f1560d7ff09dcec4cebba28ae904e9ea741a3e"
BTBN_LINUX_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-06-06-13-18/ffmpeg-n7.1.4-9-gc06af95f12-linux64-gpl-7.1.tar.xz"
BTBN_LINUX_SHA="b325724cc95caa74e56bc755e99ba182c49c2c914c650a2658635f1085c4e081"
OSX_FFMPEG_URL="https://www.osxexperts.net/ffmpeg81arm.zip"
OSX_FFMPEG_SHA="ebb82529562b71170807bbc6b0e7eb4f0b13af8cbb0e085bb9e8f6fe709598ad"
OSX_FFPROBE_URL="https://www.osxexperts.net/ffprobe81arm.zip"
OSX_FFPROBE_SHA="a6640a77d38a6f0527c5b597e599cb36a3427a6931444ed80bc62542421950a1"
# ------------------------------------------------------------------------

fetch() { # <url> <sha256> <outfile>
  curl -fL --retry 3 -o "$3" "$1"
  if command -v sha256sum >/dev/null 2>&1; then
    echo "$2  $3" | sha256sum -c -
  else
    echo "$2  $3" | shasum -a 256 -c -
  fi
}

case "$target" in
  macos-arm64)
    fetch "$OSX_FFMPEG_URL" "$OSX_FFMPEG_SHA" "$dest/ffmpeg.zip"
    fetch "$OSX_FFPROBE_URL" "$OSX_FFPROBE_SHA" "$dest/ffprobe.zip"
    unzip -oj "$dest/ffmpeg.zip" -d "$dest"
    unzip -oj "$dest/ffprobe.zip" -d "$dest"
    rm "$dest/ffmpeg.zip" "$dest/ffprobe.zip"
    chmod +x "$dest/ffmpeg" "$dest/ffprobe"
    ;;
  linux-x86_64)
    fetch "$BTBN_LINUX_URL" "$BTBN_LINUX_SHA" "$dest/ffmpeg.tar.xz"
    # Portable across GNU tar (CI) and bsdtar (local macOS verification):
    # extract fully, then keep only the two binaries.
    tar -xJf "$dest/ffmpeg.tar.xz" -C "$dest"
    topdir="$(find "$dest" -mindepth 1 -maxdepth 1 -type d)"
    mv "$topdir/bin/ffmpeg" "$topdir/bin/ffprobe" "$dest/"
    rm -rf "$topdir" "$dest/ffmpeg.tar.xz"
    ;;
  windows-x86_64)
    fetch "$BTBN_WIN_URL" "$BTBN_WIN_SHA" "$dest/ffmpeg.zip"
    7z e -y -o"$dest" "$dest/ffmpeg.zip" '*/bin/ffmpeg.exe' '*/bin/ffprobe.exe' >/dev/null
    rm "$dest/ffmpeg.zip"
    ;;
  *)
    echo "unknown target: $target" >&2; exit 1 ;;
esac

ls -lh "$dest"
