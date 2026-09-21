---
title: Procedura di release
description: Checklist per pubblicare una nuova versione di Backuppo.
---

La pipeline `.github/workflows/release.yml` si attiva sui tag `v*`. Compila e
allega binari Linux musl, Windows e macOS, genera `SHA256SUMS`, pubblica
l'immagine multiarch su GHCR e crea la release GitHub.

## Checklist

1. Aggiornare la versione workspace e `CHANGELOG.md`.
2. Verificare che il nome sia ancora disponibile su crates.io. Per `backuppo`
   il percorso dello sparse index è `https://index.crates.io/ba/ck/backuppo`.
3. Eseguire:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo build --locked --release --package backuppo
   docker build -t backuppo:release-test .
   docker run --rm backuppo:release-test --version
   ```

4. Controllare che `RELEASE_NOTES.md` descriva la versione pronta.
5. Creare e pubblicare il commit di release.
6. Creare il tag annotato e inviarlo:

   ```sh
   git tag -a v0.1.0 -m "Backuppo v0.1.0"
   git push origin main v0.1.0
   ```

La pipeline rifiuta un tag che non coincide con la versione del crate
`backuppo`. La pubblicazione su crates.io è un'operazione separata e permanente;
non è eseguita automaticamente dalla pipeline GitHub.
