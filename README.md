<div align="center">

<img src="assets/branding/backuppo-squirrel-logo.png" alt="Backuppo" width="160">

# Backuppo

**Backup verificati, su una macchina o su cento.**

Agent open source e cross-platform con restore verificato automaticamente,
cifratura `age` e Hub opzionale per gestire più siti.

[![CI](https://github.com/garzuu/backuppo/actions/workflows/ci.yml/badge.svg)](https://github.com/garzuu/backuppo/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/garzuu/backuppo?sort=semver)](https://github.com/garzuu/backuppo/releases)
[![Licenza](https://img.shields.io/badge/licenza-MIT%20%2F%20Apache--2.0-blue)](#licenza)
![Piattaforme](https://img.shields.io/badge/piattaforme-Linux%20%7C%20macOS%20%7C%20Windows%20%7C%20Docker-lightgrey)

[**Documentazione**](https://garzuu.github.io/backuppo) ·
[Installazione](https://garzuu.github.io/backuppo/installazione/) ·
[Quickstart](https://garzuu.github.io/backuppo/quickstart/) ·
[Configurazione](https://garzuu.github.io/backuppo/configuration/) ·
[Changelog](CHANGELOG.md)

</div>

---

## Perché Backuppo

Un backup che non hai mai ripristinato è solo una speranza. Backuppo esegue i
backup definiti in YAML, li cifra e **verifica automaticamente che siano
ripristinabili**, avvisandoti se qualcosa non va.

| | |
|---|---|
| **Sorgenti** | Cartelle, PostgreSQL, MySQL/MariaDB, SQLite, volumi Docker, immagini disco, VM libvirt, comandi custom |
| **Destinazioni** | Filesystem, SFTP, S3, WebDAV, Google Drive, Dropbox, OneDrive |
| **Sicurezza** | Cifratura `age` (passphrase o identità X25519), modalità Restic append-only, verifica S3 Object Lock, policy di sito firmate Ed25519 |
| **Restic** | Snapshot incrementali e deduplicati per dataset grandi, incluso nelle release |
| **Restore** | Completo o selettivo da CLI, API e Web UI, con navigazione degli snapshot, dry-run e protezione contro la sovrascrittura |
| **Notifiche** | SMTP, Telegram, ntfy, webhook, più pagina di stato generata |
| **Aggiornamenti** | Release firmate Ed25519 con canali stable/beta/pinned, anti-rollback e rollback del binario |
| **Osservabilità** | Storico SQLite locale, log per esecuzione, API e Web UI |

## Agent e Hub

Backuppo è distribuito come due componenti, per Linux, macOS, Windows e Docker:

- **Agent** (`bkpo`): gira sull'host da proteggere. Esegue i job, mantiene lo
  storico e funziona anche stand-alone.
- **Hub** (`backuppo-hub`): opzionale. Raccoglie stato, inventario e log di più
  agent, invia comandi remoti e distribuisce le policy di sicurezza firmate.
  Amministra utenti e token.

Su Windows entrambi possono essere installati come servizio.

## Avvio rapido

```sh
cp examples/config.yaml config.yaml
export BACKUP_PASSPHRASE='una-passphrase-lunga'
bkpo check --config config.yaml
bkpo run --config config.yaml --job home-documents
bkpo verify --config config.yaml --job home-documents
bkpo snapshots --config config.yaml --job home-documents
bkpo browse --config config.yaml --job home-documents --snapshot latest
bkpo restore --config config.yaml --job home-documents --snapshot latest --target ./restore
```

Per installare il componente giusto usa la
[guida di installazione](https://garzuu.github.io/backuppo/installazione/); gli
script e le configurazioni pronte sono in [`deploy/`](deploy/) (`linux`,
`macos`, `windows`, `docker`, `systemd`).

## Comandi principali

```text
bkpo check          valida la configurazione
bkpo run            esegue un backup
bkpo verify         verifica l'ultimo backup
bkpo snapshots      elenca gli snapshot disponibili
bkpo browse         sfoglia il contenuto di uno snapshot
bkpo restore        ripristina in una directory sicura
bkpo storage-check  verifica la policy S3 Object Lock
bkpo maintain       esegue retention Restic con credenziali amministrative
bkpo update         controlla e installa release firmate
bkpo daemon         avvia scheduler, retention e report
bkpo serve          avvia soltanto API e Web UI locale
bkpo runs           mostra lo storico locale
bkpo logs <id>      mostra il log di un'esecuzione
bkpo notify-test    prova i notifier configurati
bkpo service        avvia sotto Windows Service Control Manager (solo Windows)
```

Riferimento completo: [CLI](https://garzuu.github.io/backuppo/cli/).

## Web UI

La stessa interfaccia responsive è incorporata nell'agent e nell'hub. Sul
singolo host permette di eseguire e verificare job, consultare storico e log,
ripristinare snapshot e modificare la configurazione con un wizard multi-job,
validazione e reload sicuro. Sull'hub offre la vista multi-sito, i comandi
remoti e l'amministrazione di utenti e token. È installabile come PWA ed è
disponibile un wrapper desktop Tauri opzionale. Vedi la [guida alla UI](docs/ui.md).

## Documentazione

La documentazione completa, in italiano e in inglese, è su
**[garzuu.github.io/backuppo](https://garzuu.github.io/backuppo)**:

- [Come funziona](https://garzuu.github.io/backuppo/come-funziona/) e [concetti](https://garzuu.github.io/backuppo/concetti/)
- [Sorgenti](https://garzuu.github.io/backuppo/sorgenti/) e [destinazioni](https://garzuu.github.io/backuppo/destinazioni/)
- [Restore](https://garzuu.github.io/backuppo/restore/) e [sicurezza](https://garzuu.github.io/backuppo/sicurezza/)
- [Hub](https://garzuu.github.io/backuppo/hub/) e [deploy dell'hub](https://garzuu.github.io/backuppo/hub-deploy/)
- [Aggiornamenti](https://garzuu.github.io/backuppo/updates/), [operatività](https://garzuu.github.io/backuppo/operativita/) e [troubleshooting](https://garzuu.github.io/backuppo/troubleshooting/)

## Licenza

Distribuito con doppia licenza [MIT](LICENSE-MIT) oppure
[Apache-2.0](LICENSE-APACHE).
