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
