# Changelog

Tutte le modifiche rilevanti sono documentate in questo file.

## [Unreleased]

- Motore Restic opzionale per snapshot incrementali e deduplicati.
- API e Web UI locale con stato, storico e avvio manuale dei job.
- Destinazioni Google Drive, Dropbox e OneDrive tramite OAuth.
- Sorgenti per immagini disco e VM libvirt spente.
- Log del daemon: senza `RUST_LOG` non veniva scritto nulla (solo errori). Ora `bkpo daemon`/`serve` loggano a `info`, su stderr e senza colori ANSI quando non è un terminale; le unit systemd usano `RUST_LOG=info`.

## [0.1.0] - 2026-09-20

Prima release stabile dell'agent Backuppo.

### Funzionalità

- Backup di cartelle, PostgreSQL, MySQL/MariaDB, SQLite, volumi Docker e stdout
  di comandi custom.
- Archivi tar con compressione zstd e cifratura `age` tramite passphrase.
- Destinazioni filesystem, SFTP, S3 compatibile e WebDAV, con retry e limite di
  banda configurabile.
- Verifica automatica del restore, inclusi restore database in container
  temporanei e query di sanità.
- Scheduler daemon, lock per job, retention giornaliera/settimanale/mensile e
  shutdown pulito.
- Notifiche Telegram, SMTP e webhook, più report periodici.
- Storico SQLite, log consultabili da CLI e pagina HTML statica di stato.
- Binari Linux statico musl, Windows e macOS, immagine container, unit systemd e
  servizio Windows nativo.

### Comandi

- `bkpo check`, `run`, `verify`, `daemon`, `notify-test`, `runs` e `logs`.
