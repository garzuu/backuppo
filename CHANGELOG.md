# Changelog

Tutte le modifiche rilevanti sono documentate in questo file.

## [Unreleased]

- Nuovo `bkpo mcp`: server MCP in sola lettura su stdio (`list_jobs`,
  `failing_jobs`, `job_history`) per interrogare lo stato dei backup da
  Claude Desktop/Code o altri client MCP. Nessun tool di scrittura.
- Nuovo endpoint `GET /metrics` in formato Prometheus (storico esecuzioni
  e verifiche per job: timestamp, esito, durata, byte).
- Nuovo `bkpo restore-db`: ripristina un dump Postgres/MySQL direttamente in
  un container gia' in esecuzione o in un server raggiungibile dall'agent,
  invece di limitarsi a estrarlo in una directory locale. Sovrascrive con
  `pg_restore --clean --if-exists`/il comportamento di default di
  `mysqldump` solo gli oggetti presenti nel dump. Testato end-to-end con un
  vero server PostgreSQL locale, incluso il caso di conflitto di schema.
- Nuovo `bkpo keys`: genera identita' age X25519 (`generate-age`) e ruota le
  password dei repository Restic senza ricifrare nulla (`rotate-restic`,
  `list-restic`, `remove-restic`), sfruttando la gestione chiavi nativa di
  Restic.
- Pacchetti `.deb`/`.rpm` automatici a ogni release; template per Homebrew,
  AUR e winget da compilare con `scripts/packaging/render-templates.sh`.
  Pubblicazione opzionale anche su Docker Hub oltre a GHCR.
- Wizard: gestione completa dei notifier (crea/modifica/elimina telegram,
  smtp, webhook, ntfy) e campi avanzati per le destinazioni (retry,
  limite di banda, virtual-host-style S3, inizializzazione repository
  Restic).
- Il wizard crea repository e job Restic per default, con binario Restic 0.19.1
  incluso nelle release e nell'immagine container.
- Ripristino completo o selettivo da CLI, API e Web UI, con navigazione degli
  snapshot, dry-run e protezione esplicita contro la sovrascrittura.
- Aggiornamenti firmati Ed25519 con canali stable/beta/pinned, verifica di hash e
  dimensione, protezione anti-rollback, staging atomico e rollback del binario.
- Modalita Restic append-only, che impedisce all'agent di eseguire retention e
  prune sul repository protetto.
- Verifica online S3 Object Lock e manutenzione Restic manuale con credenziali
  amministrative isolate da quelle usate dal daemon.
- Cifratura age con identita X25519 letta da variabile d'ambiente, oltre alla
  passphrase gia supportata.
- Inventario agent nel hub con versione, piattaforma e capability, mantenendo la
  compatibilita con gli agent precedenti.
- Policy di sicurezza per sito firmate Ed25519, con scadenza, anti-rollback e
  modalita audit/block applicata a scheduler, UI e comandi remoti.
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
