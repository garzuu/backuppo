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

## Firma degli aggiornamenti

Il job release richiede il secret GitHub `UPDATE_SIGNING_KEY_B64`, contenente
una chiave privata Ed25519 PEM codificata base64. La chiave si genera una sola
volta e va conservata offline:

```sh
openssl genpkey -algorithm ED25519 -out update-signing-key.pem
base64 < update-signing-key.pem
openssl pkey -in update-signing-key.pem -pubout -outform DER | tail -c 32 | base64
```

L'ultimo valore è `updates.public_key` e va distribuito separatamente dagli
artefatti. La pipeline pubblica binari raw, `update-stable.json` e la relativa
firma; una chiave mancante fa fallire la release.
