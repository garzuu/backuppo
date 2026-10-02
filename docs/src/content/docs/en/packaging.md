---
title: System packages
description: How the release pipeline generates .deb, .rpm and the templates for Homebrew, winget and AUR.
---

Besides the raw tarballs/zips and the Docker image, the release pipeline
produces native Linux packages and ready-to-fill templates for the package
managers that require a separate publication step.

## .deb and .rpm (automatic)

The `packages-linux` job in `.github/workflows/release.yml` builds the agent
and the hub with `cargo-deb` and `cargo-generate-rpm` (metadata lives in
`crates/cli/Cargo.toml` and `crates/hub/Cargo.toml`) and attaches to the
GitHub release:

- `backuppo-agent-<version>-amd64.deb` / `backuppo-hub-<version>-amd64.deb`
- `backuppo-agent-<version>-x86_64.rpm` / `backuppo-hub-<version>-x86_64.rpm`

They install the binary under `/usr/bin`, docs under `/usr/share/doc/`, and
the systemd unit (disabled by default: enable it with
`systemctl enable --now backuppo` after writing
`/etc/backuppo/config.yaml`). No external publication is required: they are
attached to the release like the tarballs.

## Homebrew, AUR, winget (manual)

These three ecosystems live in third-party repositories (a Homebrew tap,
`aur.archlinux.org`, `microsoft/winget-pkgs`) and each requires its own
account and publication step that cannot be automated from this pipeline.
The repository only keeps the **templates** under
`deploy/packaging/{homebrew,aur,winget}/`.

After a release is published, render the filled-in files with:

```sh
scripts/packaging/render-templates.sh 0.2.0
```

The script downloads `SHA256SUMS` from the matching GitHub release and
produces, under `dist/packaging/0.2.0/`:

- `homebrew/backuppo.rb` — copy into your own tap (e.g.
  `garzuu/homebrew-backuppo`); Homebrew core has separate notability
  criteria and its own PR process.
- `aur/PKGBUILD` — copy into a checkout of
  `ssh://aur@aur.archlinux.org/backuppo-bin.git`, then run
  `makepkg --printsrcinfo > .SRCINFO` and push.
- `winget/garzuu.Backuppo*.yaml` — open as a PR against
  `microsoft/winget-pkgs`, under
  `manifests/g/garzuu/Backuppo/<version>/`.

None of these three steps run in CI: they are publications made under the
maintainer's own account on external infrastructure.
