# ROADMAP — Backup tool open source (stile Iperius) in Rust

Nome provvisorio: `backupper`

## Come usare questo file con Claude Code

- Lavora **una fase alla volta**, nell'ordine. Non anticipare fasi successive.
- Ogni task ha un criterio di completamento: spuntalo solo se i test passano.
- Prima di scrivere codice in una fase, leggi le "Decisioni prese" qui sotto: non rimetterle in discussione senza chiedere.
- Dopo ogni fase: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, poi commit.
- Se una decisione cambia, aggiorna questo file.

## Obiettivo

Tool di backup generico, cross-platform (Linux/Windows/macOS), open source, distribuito come **singolo binario statico**. Configurabile via YAML versionabile su git. Notifiche via SMTP e Telegram. Differenziatore principale: **verifica automatica del restore**.

Modello di un job: `sorgente → (compressione + cifratura) → destinazione → notifiche`

## Decisioni prese

- Linguaggio: **Rust** (edition 2021), workspace con più crate.
- Motore MVP: **proprio** (tar + zstd + cifratura `age`), non restic. Un motore incrementale/dedup verrà aggiunto dopo come secondo `Engine`.
- Storage: **opendal** (fs, sftp, s3, webdav) dietro il trait `Destination`.
- TLS: solo **rustls** (mai OpenSSL, per facilitare il build statico musl).
- Config: **YAML** con `serde_yaml`. I segreti non stanno nel file: si referenziano via `*_env`.
- Async: `tokio`. Errori: `thiserror` nelle librerie, `anyhow` solo nel crate `cli`.
- Fuori scope per l'MVP: immagini disco, VM, Exchange, VSS, GUI.

## Struttura del workspace

```
backupper/
├── Cargo.toml              # workspace
├── ROADMAP.md
├── crates/
│   ├── core/               # trait, tipi, errori, config (nessuna dipendenza interna)
│   ├── engine/             # esecuzione job, retention, verifica restore
│   ├── sources/            # folder, postgres, mysql, sqlite, docker-volume, command
│   ├── destinations/       # local, sftp, s3, webdav (via opendal)
│   ├── notifiers/          # smtp, telegram, webhook
│   └── cli/                # binario: clap, scheduler, logging
└── examples/config.yaml
```

Regola: `core` non dipende da nessun altro crate del workspace; tutti gli altri dipendono da `core`.

## Trait di riferimento (in `core`)

- `Source`: `prepare(staging) -> Artifact`, `cleanup(artifact)`
- `Destination`: `upload`, `download`, `list`, `delete`
- `Notifier`: `send(JobEvent)`
- `JobEvent`: `Success`, `Failure`, `RestoreVerified`, `Report`
- `Artifact`: `path`, `bytes`, `files`, `checksum`

---

## Fase 0 — Setup

- [x] Workspace Cargo con i 6 crate vuoti che compilano
- [x] CI GitHub Actions: fmt, clippy, test (Linux)
- [x] `LICENSE` (MIT + Apache-2.0 dual), `README.md` di 5 righe che spiega il problema
- [x] `examples/config.yaml` con lo schema target

**Fatto quando:** `cargo build` e `cargo test` passano sul workspace vuoto e la CI è verde.

## Fase 1 — Core e config

- [ ] Trait e tipi in `core` (vedi sopra) + `BackupError`
- [ ] Parser config con `serde`: `notifiers`, `destinations`, `jobs`
- [ ] Validazione: riferimenti a destination/notifier inesistenti, campi mancanti, cron non valido → errori chiari con nome del campo
- [ ] Risoluzione segreti da variabili d'ambiente (`password_env`, `token_env`)
- [ ] Test unitari su config valide e non valide

**Fatto quando:** `backupper check --config config.yaml` valida il file e stampa errori leggibili.

## Fase 2 — Primo giro end to end (cartella → locale)

- [ ] `Source` cartella (staging, conteggio file/byte, esclusioni glob)
- [ ] Engine: tar + zstd + cifratura `age` (chiave/passphrase da env)
- [ ] `Destination` locale (opendal fs)
- [ ] Comando `backupper run --config c.yaml --job <nome>`
- [ ] Logging strutturato con `tracing`
- [ ] Test di integrazione: backup di una cartella temporanea e confronto contenuto

**Fatto quando:** un job cartella → directory locale produce un archivio cifrato e il test lo ripristina identico.

## Fase 3 — Verifica restore (il differenziatore)

- [ ] Comando `backupper verify --job <nome>`: scarica l'ultimo backup, decifra, decomprime in dir temporanea
- [ ] Controlli per cartelle: numero di file, dimensioni, checksum di un campione
- [ ] Config `verify_restore: never | every | daily | weekly` per job
- [ ] Evento `RestoreVerified` con dettaglio
- [ ] Allarme "backup troppo vecchio" (soglia configurabile)

**Fatto quando:** un backup corrotto di proposito (byte alterato) fa fallire la verifica con messaggio chiaro.

## Fase 4 — Notifiche

- [ ] Notifier Telegram (reqwest + rustls, chiamata `sendMessage`)
- [ ] Notifier SMTP (`lettre`, STARTTLS e TLS implicito)
- [ ] Regole per job: `on_success`, `on_failure`, `on_verify`
- [ ] Comando `backupper notify-test` per provare i canali
- [ ] Test con mock server per Telegram e SMTP locale

**Fatto quando:** un job fallito manda mail e Telegram, un job riuscito solo Telegram, come da config.

## Fase 5 — Scheduler e retention

- [ ] Modalità daemon: `backupper daemon` con scheduler cron-like (`tokio-cron-scheduler`)
- [ ] Un job non parte se il precedente è ancora in esecuzione (lock)
- [ ] Retention `daily/weekly/monthly` con cancellazione dei backup scaduti
- [ ] Retention **mai** distruttiva se l'ultimo backup è fallito o non verificato
- [ ] Shutdown pulito su SIGTERM/Ctrl+C

**Fatto quando:** il daemon esegue due job schedulati e applica la retention senza cancellare l'unico backup valido.

## Fase 6 — Destinazioni remote

- [ ] SFTP (Hetzner Storage Box come caso di test reale)
- [ ] S3-compatibili (B2, Wasabi, MinIO) con test su MinIO in Docker
- [ ] WebDAV
- [ ] Retry con backoff e upload resumable dove possibile
- [ ] Limite di banda opzionale

**Fatto quando:** lo stesso job funziona verso locale, SFTP e S3 cambiando solo la destination.

## Fase 7 — Sorgenti database e Docker

- [ ] Postgres (`pg_dump`, anche dentro container via Docker)
- [ ] MySQL/MariaDB (`mysqldump`)
- [ ] SQLite (backup consistente con `.backup`)
- [ ] Volumi Docker (`bollard`)
- [ ] Sorgente "comando custom" (stdout) e hook `pre`/`post`
- [ ] Verifica restore per DB: caricare il dump in un container temporaneo ed eseguire una query di sanità

**Fatto quando:** un dump Postgres viene ripristinato in un container usa e getta e la query di controllo passa.

## Fase 8 — Report e osservabilità

- [ ] Report periodico via mail (es. "7 job ok, 1 fallito, ultimo restore test: ieri")
- [ ] Webhook generico (copre Slack, Discord, ntfy)
- [ ] Pagina di stato HTML statica generata a ogni run
- [ ] Storico esecuzioni (SQLite locale o file JSON)

**Fatto quando:** arriva il report settimanale con lo stato reale di ogni job.

## Fase 9 — Release

- [ ] Build statici Linux (musl), Windows e macOS via CI
- [ ] Immagine Docker minimale
- [ ] Servizio systemd d'esempio e installer Windows (servizio)
- [ ] Documentazione: quickstart, riferimento config, guida al restore manuale
- [ ] Release `v0.1.0` con changelog

## Fase 10 — Dopo l'MVP (da valutare)

- [ ] Motore incrementale/dedup (chunking, oppure integrazione restic)
- [ ] Web UI leggera sopra un'API locale (stato, log, avvio manuale)
- [ ] Google Drive / Dropbox / OneDrive (via opendal)
- [ ] Immagini disco / VM (solo se c'è domanda reale)

---

## Regole di qualità (valgono sempre)

- Nessun `unwrap()`/`expect()` fuori dai test: ogni errore va propagato o loggato.
- Mai loggare segreti, token, password o contenuto dei backup.
- Un backup si considera riuscito **solo** dopo che l'upload è confermato e (se attivo) il restore è verificato.
- Ogni nuova sorgente/destinazione/notifier implementa il trait di `core` e ha almeno un test di integrazione.
- Errori sempre con contesto: quale job, quale fase, quale risorsa.

## Rischi da tenere d'occhio

- Allargare troppo lo scope prima che il nucleo (Fase 2–3) funzioni.
- Complessità di async e trait object: preferisci soluzioni semplici (`Box<dyn Trait>` con `async_trait`).
- Cross-compile Windows/macOS: verifica in CI fin dalle prime fasi, non alla fine.