---
title: Pacchetti di sistema
description: Come la pipeline di release genera .deb, .rpm e i template per Homebrew, winget e AUR.
---

Oltre ai tarball/zip grezzi e all'immagine Docker, la pipeline di release
produce pacchetti nativi per Linux e template pronti per i package manager
che richiedono una pubblicazione separata.

## .deb e .rpm (automatici)

Il job `packages-linux` di `.github/workflows/release.yml` compila l'agent e
l'hub con `cargo-deb` e `cargo-generate-rpm` (metadata in
`crates/cli/Cargo.toml` e `crates/hub/Cargo.toml`) e allega alla release
GitHub:

- `backuppo-agent-<versione>-amd64.deb` / `backuppo-hub-<versione>-amd64.deb`
- `backuppo-agent-<versione>-x86_64.rpm` / `backuppo-hub-<versione>-x86_64.rpm`

Installano il binario in `/usr/bin`, la documentazione in
`/usr/share/doc/` e l'unit systemd (disabilitata di default: va attivata con
`systemctl enable --now backuppo` dopo aver scritto `/etc/backuppo/config.yaml`).
Nessuna pubblicazione esterna è richiesta: sono allegati alla release come i
tarball.

## Homebrew, AUR, winget (manuali)

Questi tre ecosistemi vivono in repository di terzi (un tap Homebrew,
`aur.archlinux.org`, `microsoft/winget-pkgs`) e richiedono un account e una
pubblicazione a parte, non automatizzabile dalla pipeline. Il repository
mantiene solo i **template** in `deploy/packaging/{homebrew,aur,winget}/`.

Dopo aver pubblicato una release, generare i file compilati con:

```sh
scripts/packaging/render-templates.sh 0.2.0
```

Lo script scarica `SHA256SUMS` dalla release GitHub corrispondente e produce
in `dist/packaging/0.2.0/`:

- `homebrew/backuppo.rb` — da copiare nel tap personale (es.
  `garzuu/homebrew-backuppo`); Homebrew core richiede criteri di notabilità
  separati e una PR dedicata.
- `aur/PKGBUILD` — da copiare in un checkout di
  `ssh://aur@aur.archlinux.org/backuppo-bin.git`, poi
  `makepkg --printsrcinfo > .SRCINFO` e push.
- `winget/garzuu.Backuppo*.yaml` — da aprire come PR su
  `microsoft/winget-pkgs`, percorso
  `manifests/g/garzuu/Backuppo/<versione>/`.

Nessuno di questi tre passaggi viene eseguito in CI: sono pubblicazioni a
nome del maintainer su infrastrutture esterne.
