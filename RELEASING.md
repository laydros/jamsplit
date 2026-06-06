# Releasing jamsplit

Releases are cut by pushing a version tag. CI builds the three bundles and
creates a **draft** GitHub Release; nothing is public until the draft is
published by hand.

## Cutting a release

1. Decide the version (`X.Y.Z`, no `v`). Update `version` in the root
   `Cargo.toml` if it changed, run `cargo build` to refresh `Cargo.lock`,
   commit, and push to `main`.
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

If the Release run shows the `release` job as *skipped* (gray, not red),
one of the three build legs failed — the draft was deliberately not
created. Do not read a skipped release job as success.

## If something fails after tagging

Fix the problem on `main` first. Then, **if a draft release was already
created** (the run got far enough), delete it before retrying — the
workflow cannot create a second release for the same tag:

```bash
gh release delete vX.Y.Z --yes
```

(If the `release` job was skipped because a build leg failed, no draft
exists and there is nothing to delete.)

Then move the tag and re-push:

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

OSXExperts publishes no checksums and has been observed serving different
content under an unchanged URL. A bump there is only done when the
downloaded binaries have been run locally (`ffmpeg -version`) — never
assume the URL is immutable.
