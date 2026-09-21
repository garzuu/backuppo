---
title: Release process
description: Checklist for publishing a new Backuppo version.
---

The `.github/workflows/release.yml` pipeline triggers on `v*` tags. It
builds and attaches Linux musl, Windows and macOS binaries, generates
`SHA256SUMS`, publishes the multiarch image to GHCR and creates the
GitHub release.

## Checklist

1. Update the workspace version and `CHANGELOG.md`.
2. Check that the name is still available on crates.io. For `backuppo`
   the sparse index path is `https://index.crates.io/ba/ck/backuppo`.
3. Run:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo build --locked --release --package backuppo
   docker build -t backuppo:release-test .
   docker run --rm backuppo:release-test --version
   ```

4. Check that `RELEASE_NOTES.md` describes the version being released.
5. Create and push the release commit.
6. Create the annotated tag and push it:

   ```sh
   git tag -a v0.1.0 -m "Backuppo v0.1.0"
   git push origin main v0.1.0
   ```

The pipeline rejects a tag that does not match the `backuppo` crate's
version. Publishing to crates.io is a separate, permanent operation; it
is not run automatically by the GitHub pipeline.
