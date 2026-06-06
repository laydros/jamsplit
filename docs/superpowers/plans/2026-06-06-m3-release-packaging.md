# M3 — Release Packaging CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tag push (`v*`) produces a draft GitHub Release with three batteries-included bundles — macOS arm64 (with a double-clickable `.app`), Linux x86_64, Windows x86_64 — each carrying static `ffmpeg`/`ffprobe` sidecars, so Jason can hand a download link to anyone.

**Architecture:** Two shell scripts own all packaging logic (`scripts/fetch-sidecars.sh` downloads pinned ffmpeg builds with sha256 verification; `scripts/package.sh` stages and archives per-OS bundles, including the macOS `.app` assembly). The release workflow is a thin matrix that calls them and uploads the results; a separate job creates a **draft** release from the artifacts. Scripts being workflow-independent means the macOS path is fully testable on Jason's machine before any CI run.

**Tech Stack:** GitHub Actions, bash, `gh` CLI (preinstalled on runners), BtbN FFmpeg-Builds (win/linux sidecars), OSXExperts.net (macOS arm64 sidecars), `ditto`/`codesign` (macOS bundle).

**Read first:** `docs/superpowers/specs/2026-06-05-jamsplit-design.md` — the "M3 — distribution" paragraph and the ffmpeg resolution-order rationale (sidecars adjacent to our executable need zero code changes).

**Decisions locked in by this plan (settled with Jason 2026-06-06):**

- Targets: macOS arm64, Linux x86_64, Windows x86_64. Native builds on `macos-latest` / `ubuntu-latest` / `windows-latest` — no cross-compilation.
- No code signing or notarization. macOS gets ad-hoc signatures (required for arm64 to launch at all) plus documented right-click-open / `xattr` workarounds. Windows users click through SmartScreen.
- Batteries-included bundles only; no "lite" variant.
- First release is **v0.1.0**, matching the existing workspace version. No version bump needed.
- Releases are created as **drafts**: CI builds and attaches everything, Jason reviews the auto-generated notes and clicks Publish. Nothing is public until then.
- Asset names must satisfy the landing page's matcher (`index.html`): per-OS regexes key on `windows`/`macos`/`linux` substrings, and a `cli|source` substring would *deprioritize* an asset. Names used here: `jamsplit-<version>-macos-arm64.zip`, `jamsplit-<version>-linux-x86_64.tar.gz`, `jamsplit-<version>-windows-x86_64.zip`.
- macOS bundle layout: sidecars live inside `jamsplit.app/Contents/MacOS/` (adjacent to `jamsplit-gui`, so the lookup finds them). The CLI binary ships at the zip root without its own sidecar copies — `README.txt` shows the `--ffmpeg-path` one-liner pointing into the app. This avoids doubling ~150 MB of sidecars.
- Sidecar pins (exact URL + sha256) live in `scripts/fetch-sidecars.sh` as the single source of truth. Bump procedure documented in `RELEASING.md`.
- App icon: the master is `assets/jamsplit-icon.png` (1300×1300 RGBA; also feeds the landing-page favicons in `assets/icons/`). Platform artifacts (`.icns`, `.ico`) are generated **once locally and committed** — CI never needs icon tooling. The GUI gets a runtime window/taskbar icon on all platforms (embedding the web `icon-512.png`), the Windows exe gets an embedded resource icon, and the macOS `.app` gets `CFBundleIconFile`. The CLI binary ships without an embedded icon (terminal tool — not worth a build script).

**Known wrinkle to preserve in docs:** browsers quarantine downloaded zips; on macOS every extracted file inherits the xattr. Right-click-open approves the *app*, but the first ffmpeg spawn can still be blocked as an unidentified binary on recent macOS. `README.txt` documents `xattr -dr com.apple.quarantine <extracted folder>` as the fix-everything fallback. This friction is the accepted cost of skipping notarization.

---

### Task 1: GUI footer — version and website link

**Files:**

- Modify: `crates/jamsplit-gui/src/app.rs` (the `eframe::App::update` impl, around line 317)

The GUI currently displays no version anywhere. Add a footer with the version and a link to the project site. Per the design doc, UI rendering code has no unit tests — verification is manual.

- [ ] **Step 1: Add the bottom panel**

In `app.rs`, inside `fn update`, egui requires side panels to be added **before** `CentralPanel`. Change:

```rust
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        egui::CentralPanel::default().show(ctx, |ui| {
```

to:

```rust
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("jamsplit {}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.hyperlink_to("Website", "https://laydros.github.io/jamsplit/");
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
```

- [ ] **Step 2: Verify manually**

Run: `cargo run -p jamsplit-gui`
Expected: footer strip along the bottom reading `jamsplit 0.1.0 | Website`; clicking Website opens the landing page in the default browser; footer stays visible in Setup, Exporting, and Done phases (drive an export with any wav + marker file to confirm it doesn't collide with the Done screen's buttons).

- [ ] **Step 3: fmt, clippy, test**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clean, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/jamsplit-gui/src/app.rs
git commit -m "Add version and website link to the GUI footer"
```

### Task 2: Pre-packaging housekeeping — strip release binaries, loosen eframe pin

**Files:**

- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/jamsplit-gui/Cargo.toml`

TODO.md lists loosening the eframe pin before M3 packaging. Stripping symbols shrinks release binaries substantially (egui binaries are tens of MB unstripped) — cheap win before we start shipping them.

- [ ] **Step 1: Add a release profile to the workspace root `Cargo.toml`**

Append:

```toml
[profile.release]
strip = "symbols"
```

- [ ] **Step 2: Loosen the eframe pin**

In `crates/jamsplit-gui/Cargo.toml`, change:

```toml
eframe = "0.33.3"
```

to:

```toml
eframe = "0.33"
```

Run: `cargo update -p eframe` — expected to stay on a 0.33.x version (0.34 needs rustc 1.92; if `Cargo.lock` jumps minor versions, stop and check the resolved version).

- [ ] **Step 3: Verify the workspace still builds and tests pass**

Run: `cargo test --workspace && cargo build --release -p jamsplit-cli -p jamsplit-gui`
Expected: all pass; note the binary sizes in `target/release/` (`ls -lh target/release/jamsplit target/release/jamsplit-gui`) for the README later if interesting.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/jamsplit-gui/Cargo.toml Cargo.lock
git commit -m "Strip release binaries and loosen the eframe pin to 0.33"
```

### Task 3: Platform icons — generated artifacts, window icon, exe icon

**Files:**

- Create: `assets/icons/jamsplit.icns` (generated, committed)
- Create: `assets/icons/jamsplit.ico` (generated, committed)
- Create: `crates/jamsplit-gui/build.rs`
- Modify: `crates/jamsplit-gui/Cargo.toml`
- Modify: `crates/jamsplit-gui/src/main.rs`

- [ ] **Step 1: Generate the `.icns` (one-time, macOS tooling)**

```bash
iconset="$(mktemp -d)/jamsplit.iconset"
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/jamsplit-icon.png --out "$iconset/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d assets/jamsplit-icon.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o assets/icons/jamsplit.icns
file assets/icons/jamsplit.icns
```

Expected: `Mac OS X icon, ...` and the file is a few hundred KB.

- [ ] **Step 2: Generate the `.ico` (one-time, ImageMagick)**

```bash
magick assets/jamsplit-icon.png -define icon:auto-resize=256,128,64,48,32,16 assets/icons/jamsplit.ico
file assets/icons/jamsplit.ico
```

Expected: `MS Windows icon resource - 6 icons`.

- [ ] **Step 3: Runtime window icon (all platforms)**

Add the `image` crate to the GUI for PNG decoding:

```bash
cargo add image --no-default-features --features png -p jamsplit-gui
```

In `crates/jamsplit-gui/src/main.rs`, change the whole file to:

```rust
#![windows_subsystem = "windows"]

/// Decode the embedded icon for the window/taskbar/dock. The `.icns`/`.ico`
/// artifacts cover Finder and Explorer; this covers the running window.
fn load_icon() -> eframe::egui::IconData {
    let image = image::load_from_memory(include_bytes!("../../../assets/icons/icon-512.png"))
        .expect("embedded icon is a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    eframe::egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([760.0, 560.0])
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "jamsplit",
        options,
        Box::new(|cc| {
            // One tick larger than egui's defaults for legibility.
            cc.egui_ctx.all_styles_mut(|style| {
                for font_id in style.text_styles.values_mut() {
                    font_id.size += 1.0;
                }
            });
            Ok(Box::new(jamsplit_gui::app::JamsplitApp::new()))
        }),
    )
}
```

(`with_icon` takes `impl Into<Arc<IconData>>`, so passing `IconData` directly is fine. If the compiler disagrees on the egui 0.33 signature, wrap in `std::sync::Arc::new(...)` — adapt minimally.)

- [ ] **Step 4: Windows exe icon (embedded resource)**

Append to `crates/jamsplit-gui/Cargo.toml`:

```toml
[target.'cfg(windows)'.build-dependencies]
winresource = "0.1"
```

Create `crates/jamsplit-gui/build.rs`:

```rust
// Embeds the exe icon on Windows. Host and target are always the same
// platform in this project (no cross-compilation), so cfg(windows) on the
// build script is equivalent to targeting Windows.
#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/jamsplit.ico");
    winresource::WindowsResource::new()
        .set_icon("../../assets/icons/jamsplit.ico")
        .compile()
        .expect("embedding the Windows icon resource failed");
}

#[cfg(not(windows))]
fn main() {}
```

- [ ] **Step 5: Verify locally**

Run: `cargo run -p jamsplit-gui`
Expected: the jamsplit icon appears in the dock (macOS) while running. Then `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` — clean (the no-op build.rs must not break non-Windows builds). Windows embedding is verified by the workflow dry-run later (check the exe icon in the downloaded artifact on any Windows box, or trust `file`/`7z l` showing the resource section grew).

- [ ] **Step 6: Commit**

```bash
git add assets/icons/jamsplit.icns assets/icons/jamsplit.ico \
  crates/jamsplit-gui/build.rs crates/jamsplit-gui/Cargo.toml \
  crates/jamsplit-gui/src/main.rs Cargo.lock
git commit -m "Add platform icons: window icon, Windows exe resource, icns artifact"
```

### Task 4: Sidecar fetch script with pinned, checksummed sources

**Files:**

- Create: `scripts/fetch-sidecars.sh`

This script is the single source of truth for which ffmpeg builds we ship. URLs and sha256 hashes are pinned; CI and local runs verify integrity before anything gets bundled.

- [ ] **Step 1: Resolve the current pins (one-time, values go into the script)**

The plan cannot hardcode these — they change upstream. Resolve them now and write the real values into the script in Step 2:

```bash
# BtbN (windows + linux): pick the newest dated autobuild release
gh release list -R BtbN/FFmpeg-Builds --limit 3
# List its release-branch GPL assets (NOT the master-branch ones):
gh release view <autobuild-tag> -R BtbN/FFmpeg-Builds --json assets \
  --jq '.assets[].name' | grep -E '^ffmpeg-n[0-9.]+-latest-(win64|linux64)-gpl-[0-9.]+\.(zip|tar\.xz)$'
# Download each and record its sha256:
curl -fLO <asset-url> && shasum -a 256 <file>
```

For macOS arm64, open https://www.osxexperts.net and copy the current **arm64** ffmpeg and ffprobe zip URLs (separate zips per tool), then download and `shasum -a 256` each. OSXExperts publishes no checksums, so ours pin whatever we verified by hand today.

Record all six values (3 platforms × URL + sha256; macOS has 2 URLs + 2 hashes, BtbN one archive each containing both tools).

- [ ] **Step 2: Write `scripts/fetch-sidecars.sh`**

```bash
#!/usr/bin/env bash
# Download and verify the static ffmpeg/ffprobe sidecars bundled into release
# zips. Pins (URL + sha256) are the single source of truth for what we ship;
# bump procedure is in RELEASING.md.
set -euo pipefail

target="${1:?usage: fetch-sidecars.sh <macos-arm64|linux-x86_64|windows-x86_64>}"
dest="sidecars/$target"
mkdir -p "$dest"

# --- pins (resolved 2026-06-06; see RELEASING.md to bump) ---------------
BTBN_WIN_URL="<FILL: BtbN win64-gpl zip URL>"
BTBN_WIN_SHA="<FILL: sha256>"
BTBN_LINUX_URL="<FILL: BtbN linux64-gpl tar.xz URL>"
BTBN_LINUX_SHA="<FILL: sha256>"
OSX_FFMPEG_URL="<FILL: osxexperts arm64 ffmpeg zip URL>"
OSX_FFMPEG_SHA="<FILL: sha256>"
OSX_FFPROBE_URL="<FILL: osxexperts arm64 ffprobe zip URL>"
OSX_FFPROBE_SHA="<FILL: sha256>"
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
```

Replace every `<FILL: ...>` with the values from Step 1. `chmod +x` then `chmod 755 scripts/fetch-sidecars.sh`.

- [ ] **Step 3: Verify locally (macOS path)**

Run: `scripts/fetch-sidecars.sh macos-arm64 && sidecars/macos-arm64/ffmpeg -version | head -1 && sidecars/macos-arm64/ffprobe -version | head -1`
Expected: checksum `OK` lines, then both tools print their version banner. Also run `scripts/fetch-sidecars.sh linux-x86_64` — download and extraction succeed on macOS (binaries won't run here; that's fine, CI proves them).

- [ ] **Step 4: Add `sidecars/` to `.gitignore`**

Append `/sidecars` to `.gitignore` (create the file if the repo has none; check first with `cat .gitignore`).

- [ ] **Step 5: Commit**

```bash
git add scripts/fetch-sidecars.sh .gitignore
git commit -m "Add pinned sidecar fetch script for static ffmpeg builds"
```

### Task 5: Bundle documents — NOTICE and README templates

**Files:**

- Create: `packaging/NOTICE.txt`
- Create: `packaging/README.txt`

These ship verbatim inside every bundle. NOTICE covers GPL source obligations for the bundled ffmpeg; README is the two-minute quickstart for someone who just downloaded a zip.

- [ ] **Step 1: Write `packaging/NOTICE.txt`**

```text
Third-party software notice
===========================

This jamsplit bundle includes unmodified static builds of ffmpeg and
ffprobe (https://ffmpeg.org), licensed under the GNU GPL version 2 or
later with GPL v3 components enabled. See https://ffmpeg.org/legal.html.

Build provenance:

- Windows and Linux bundles: BtbN FFmpeg-Builds (GPL variant),
  https://github.com/BtbN/FFmpeg-Builds
  Build scripts and exact configuration are published in that repository;
  the bundled build's version is printed by `ffmpeg -version`.
- macOS bundles: OSXExperts.net static builds,
  https://www.osxexperts.net

Corresponding source code for ffmpeg is available at
https://ffmpeg.org/download.html#get-sources (select the release matching
`ffmpeg -version`).

jamsplit itself is licensed under the GPL-3.0; see the bundled LICENSE
file. jamsplit source: https://github.com/laydros/jamsplit
```

- [ ] **Step 2: Write `packaging/README.txt`**

```text
jamsplit — split one long jam recording into per-song MP3s
==========================================================

Full guide with screenshots: https://laydros.github.io/jamsplit/

Quick start
-----------
1. Unpack this archive anywhere.
2. Run the GUI:
   - Windows: jamsplit-gui.exe
   - macOS:   right-click jamsplit.app, choose Open (first launch only)
   - Linux:   ./jamsplit-gui
3. Pick your recording and your marker file, check the preview, click
   Split. ffmpeg is included in this bundle - nothing else to install.

macOS: if export fails with an ffmpeg error on first run, clear the
download quarantine and retry:

    xattr -dr com.apple.quarantine <this folder>

Windows: SmartScreen may warn because the app is not code-signed.
Choose "More info" -> "Run anyway".

Command-line tool
-----------------
The jamsplit CLI is included for scripting:

    jamsplit split --audio jam.wav --markers songs.txt

On macOS the CLI does not sit next to the bundled ffmpeg; point it there
(or install ffmpeg yourself):

    ./jamsplit split --audio jam.wav --markers songs.txt \
      --ffmpeg-path ./jamsplit.app/Contents/MacOS/ffmpeg

Licensing: see LICENSE (jamsplit, GPL-3.0) and NOTICE.txt (bundled ffmpeg).
```

- [ ] **Step 3: Commit**

```bash
git add packaging/NOTICE.txt packaging/README.txt
git commit -m "Add NOTICE and README shipped inside release bundles"
```

### Task 6: Packaging script — staging, .app assembly, archives

**Files:**

- Create: `scripts/package.sh`

Consumes `target/release/` binaries plus `sidecars/<target>/`, produces `dist/jamsplit-<version>-<target>.<ext>`. The macOS branch assembles `jamsplit.app` and ad-hoc signs it (arm64 refuses to launch unsigned; ad-hoc costs nothing and needs no certificate).

- [ ] **Step 1: Write `scripts/package.sh`**

```bash
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
    mkdir -p dist
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
```

`chmod 755 scripts/package.sh`. Append `/dist` to `.gitignore`.

- [ ] **Step 2: Verify the macOS bundle end-to-end locally**

```bash
cargo build --release -p jamsplit-cli -p jamsplit-gui
scripts/fetch-sidecars.sh macos-arm64   # if sidecars/ was cleaned
scripts/package.sh macos-arm64
cd /tmp && rm -rf jamsplit-smoke && mkdir jamsplit-smoke && cd jamsplit-smoke
ditto -x -k <repo>/dist/jamsplit-0.1.0-macos-arm64.zip .
open jamsplit-0.1.0-macos-arm64/jamsplit.app
```

Expected: the app launches by double-click semantics, shows the jamsplit icon in Finder and the dock (Finder may cache the generic icon briefly; `killall Finder` if unsure), the ffmpeg status row shows the *bundled* sidecar path (inside `jamsplit.app/Contents/MacOS/`), and a real split run works. Also check `codesign --verify --deep --strict jamsplit-0.1.0-macos-arm64/jamsplit.app` exits 0, and the CLI works with the `--ffmpeg-path` line exactly as written in `packaging/README.txt`.

- [ ] **Step 3: Commit**

```bash
git add scripts/package.sh .gitignore
git commit -m "Add per-OS packaging script with macOS app bundle assembly"
```

### Task 7: Release workflow

**Files:**

- Create: `.github/workflows/release.yml`

Tag push `v*` → guard that the tag matches the Cargo version → matrix build + package → draft release with all assets. `workflow_dispatch` runs build+package only (dry run, no release), so the whole pipeline is testable without tagging.

- [ ] **Step 1: Write `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags: ["v*"]
  workflow_dispatch: # dry run: build and package, skip release creation

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    name: package (${{ matrix.target }})
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-latest
            target: macos-arm64
          - os: ubuntu-latest
            target: linux-x86_64
          - os: windows-latest
            target: windows-x86_64
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v5
      - name: Verify tag matches Cargo.toml version
        if: github.event_name == 'push'
        shell: bash
        run: |
          tag="${GITHUB_REF_NAME#v}"
          ver="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
          if [ "$tag" != "$ver" ]; then
            echo "tag v$tag does not match Cargo.toml version $ver" >&2
            exit 1
          fi
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux GUI build deps
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
      - name: Build release binaries
        run: cargo build --release --locked -p jamsplit-cli -p jamsplit-gui
      - name: Fetch sidecars
        shell: bash
        run: scripts/fetch-sidecars.sh ${{ matrix.target }}
      - name: Package
        shell: bash
        run: scripts/package.sh ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: bundle-${{ matrix.target }}
          path: dist/jamsplit-*
          if-no-files-found: error

  release:
    needs: build
    if: github.event_name == 'push'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true
      - name: Create draft release
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release create "$GITHUB_REF_NAME" dist/* \
            --repo "$GITHUB_REPOSITORY" \
            --draft \
            --generate-notes \
            --verify-tag \
            --title "jamsplit $GITHUB_REF_NAME"
```

Note for the executor: if upload/download-artifact emit Node-deprecation annotations on the first run, bump to the current major the annotation recommends — same situation as the `checkout@v5` bump in commit `5592bc5`.

- [ ] **Step 2: Commit and push, then dry-run**

```bash
git add .github/workflows/release.yml
git commit -m "Add tag-triggered release workflow producing draft releases"
```

**STOP: get Jason's go-ahead before pushing** (his rule: no pushes without explicit instruction). After pushing:

```bash
gh workflow run release.yml --repo laydros/jamsplit
gh run watch --repo laydros/jamsplit --exit-status <run-id>
```

Expected: all three matrix jobs green; no release created (dry run). Download and spot-check each artifact:

```bash
gh run download <run-id> --repo laydros/jamsplit -D /tmp/jamsplit-artifacts
ls -R /tmp/jamsplit-artifacts
```

Expected contents per bundle: binaries + ffmpeg + ffprobe + LICENSE + NOTICE.txt + README.txt (macOS: `jamsplit.app` — with `Contents/Resources/jamsplit.icns` — plus the `jamsplit` CLI). Verify the Linux tar preserves the executable bit (`tar -tvzf ... | grep ffmpeg` shows `rwxr-xr-x`). If a Windows or Linux job fails on script details (7z flags, tar wildcards), fix minimally and re-dispatch.

### Task 8: Release documentation

**Files:**

- Create: `RELEASING.md`
- Modify: `README.md` (add a Download section)
- Modify: `TODO.md`

- [ ] **Step 1: Write `RELEASING.md`**

```markdown
# Releasing jamsplit

Releases are cut by pushing a version tag. CI builds the three bundles and
creates a **draft** GitHub Release; nothing is public until the draft is
published by hand.

## Cutting a release

1. Decide the version (`X.Y.Z`, no `v`). Update `version` in the root
   `Cargo.toml` if it changed, run `cargo build` to refresh `Cargo.lock`,
   commit, and push.
2. Confirm CI is green on `main`.
3. Tag and push the tag:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

4. Watch the Release workflow (`gh run list --workflow Release`). It ends
   by creating a draft release with the three bundles attached.
5. Open https://github.com/laydros/jamsplit/releases — the draft is at the
   top. Review the auto-generated notes (every merged change since the
   last tag), edit freely, then **Publish release**.
6. Check https://laydros.github.io/jamsplit/ — the download buttons read
   the latest published release and should now resolve to the new assets.

## If the build fails after tagging

Fix the problem on `main`, then move the tag and re-push:

```bash
git tag -f vX.Y.Z
git push -f origin vX.Y.Z
```

Re-pushing a tag is fine *before* the release is published. Never move a
tag that has a published release.

## Dry run (no tag, no release)

`gh workflow run release.yml` builds and packages everything and uploads
workflow artifacts, but creates no release.

## Bumping the bundled ffmpeg

Pins live in `scripts/fetch-sidecars.sh`. Pick a new BtbN release
(`gh release list -R BtbN/FFmpeg-Builds`) and new OSXExperts zips, download
each asset, recompute `shasum -a 256`, update URL+sha pairs, and verify
with `scripts/fetch-sidecars.sh <target>` locally before committing.
```

- [ ] **Step 2: Add a Download section to `README.md`**

After the project description (read the file first to place it sensibly), add:

```markdown
## Download

Prebuilt bundles for macOS (Apple Silicon), Linux x86_64, and Windows
x86_64 — ffmpeg included — are on the
[releases page](https://github.com/laydros/jamsplit/releases/latest), or
via the [project site](https://laydros.github.io/jamsplit/).
```

- [ ] **Step 3: Update `TODO.md`**

- Under `## Next milestone`, replace the M3 line with a note that M3 is done except the post-release landing-page revisit (leave that item).
- Add a new section:

```markdown
## Future (post-M3 candidates)

- wav concat helper: Zoom recorders split long sessions at the 2/4 GB
  boundary; the design doc punts to a documented ffmpeg concat one-liner.
  A `jamsplit concat`-style helper (CLI subcommand and/or GUI affordance)
  would remove that manual step. Out of v1 scope per the design doc;
  discuss scope with Jason before starting.
```

- [ ] **Step 4: Commit**

```bash
git add RELEASING.md README.md TODO.md
git commit -m "Document the release process and downloads"
```

### Task 9: Cut v0.1.0 (Jason in the loop)

No new files — this is the first real run of the pipeline, following `RELEASING.md` exactly.

- [ ] **Step 1:** Confirm `main` is green and pushed (needs Jason's explicit OK for any push).
- [ ] **Step 2:** Tag `v0.1.0` and push the tag (Jason's OK again — this is the public-ish step, though the release stays draft).
- [ ] **Step 3:** Watch the Release workflow to completion; confirm the draft release holds all three assets with the expected names.
- [ ] **Step 4:** Jason reviews the draft notes and publishes.
- [ ] **Step 5:** Verify the landing page download buttons now resolve per-OS assets (the page's JS reads the published release). Then start the TODO.md "Landing page" revisit item as follow-up work.

---

## Self-review notes

- Spec coverage: design doc M3 paragraph = release matrix (Task 7), batteries zips + NOTICE (Tasks 4–6), `.app` bundle (Task 6). Session decisions (targets, no signing, draft releases, v0.1.0, asset naming, icons) captured in the header and Tasks 3 and 6–9. TODO.md pre-M3 items (eframe pin) in Task 2. Landing-page matcher compatibility in header + Task 9 Step 5.
- The `<FILL: ...>` markers in Task 4 are deliberate: upstream URLs/hashes must be resolved at execution time; Step 1 gives the exact commands to resolve them. Everything else is concrete.
- Push points are explicitly gated on Jason's instruction (his global rule).
