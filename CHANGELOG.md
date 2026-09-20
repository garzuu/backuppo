# Changelog

Tutte le modifiche rilevanti sono documentate in questo file.

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
