# ROADMAP — Backuppo: backup open source (stile Iperius) in Rust

Nome progetto: **Backuppo** · crate: `backuppo` · binario (comando): `bkpo`

## Come usare questo file con Claude Code

- Lavora **una fase alla volta**, nell'ordine. Non anticipare fasi successive.
- Ogni task ha un criterio di completamento: spuntalo solo se i test passano.
- Prima di scrivere codice in una fase, leggi le "Decisioni prese" qui sotto: non rimetterle in discussione senza chiedere.
- Dopo ogni fase: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, poi commit.
- Se una decisione cambia, aggiorna questo file.

## Obiettivo

Tool di backup generico, cross-platform (Linux/Windows/macOS), open source, distribuito come **singolo binario statico**. Configurabile via YAML versionabile su git. Notifiche via SMTP e Telegram. Differenziatore principale: **verifica automatica del restore**.

Modello di un job: `sorgente → (compressione + cifratura) → destinazione → notifiche`

## Architettura a tre pezzi

1. **Agent** (questo tool): gira su ogni server/sito, esegue i job, notifica. **È il prodotto principale e funziona sempre stand-alone.**
2. **Hub** (futuro, opzionale): server centrale che riceve gli stati dagli agent, controlla gli heartbeat, gestisce clienti/siti/utenti ed espone l'API.
3. **App Flutter** (futura, opzionale): client dell'hub per controllare lo stato di tutti i siti e ricevere notifiche push.

Ordine di sviluppo: prima l'agent fino a `v0.1.0` stabile, poi l'hub, poi l'app.

## Decisioni prese

- Linguaggio: **Rust** (edition 2021), workspace con più crate.
- **Nome**: progetto e repo `backuppo` (gioco di parole backup + hippo, mascotte: ippopotamo). Il crate CLI si chiama `backuppo` e definisce il binario `bkpo` con `[[bin]] name = "bkpo"` in `Cargo.toml`. Prima di pubblicare, verificare la disponibilità del crate `backuppo` su crates.io.
- Motore MVP: **proprio** (tar + zstd + cifratura `age`), non restic. Un motore incrementale/dedup verrà aggiunto dopo come secondo `Engine`.
- Storage: **opendal** (fs, s3, webdav) dietro il trait `Destination`. **SFTP fa eccezione**: il backend sftp di opendal fa da wrapper al binario di sistema `ssh` e supporta solo autenticazione a chiave, quindi per supportare anche la password si usa un client nativo in Rust (`russh` + `russh-sftp`), restando comunque un binario statico. Deciso in Fase 6, confermato dall'utente.
- TLS: solo **rustls** (mai OpenSSL, per facilitare il build statico musl).
- Config: **YAML** con `serde_yaml`. I segreti non stanno nel file: si referenziano via `*_env`.
- Async: `tokio`. Errori: `thiserror` nelle librerie, `anyhow` solo nel crate `cli`.
- Fuori scope per l'MVP: immagini disco, VM, Exchange, VSS, GUI.
- **Agent stand-alone**: l'hub è solo un `Notifier` (`type: hub`). Senza blocco `hub` nella config, l'agent non sa che esiste. Nessun crate dell'agent importa codice dell'hub.
- **Hub come binario separato** (`crates/hub`), con i soli tipi condivisi (eventi, modelli) in `core`. Il client dell'hub nell'agent sta dietro il Cargo feature flag `hub`.
- Gli agent parlano con l'hub solo in **uscita** (push), così funzionano dietro NAT/firewall. Inviano **solo metadati** (esiti, durate, errori), mai contenuto dei backup né segreti.
- API dell'hub **versionata** (`/v1/...`) fin dall'inizio.
- Modello dati hub: `Organizzazione/Cliente → Sito → Job → Esecuzione`, con ruoli (admin, sola lettura).
- App: **Flutter** (Riverpod, `dio`, `go_router`, `drift`, `fl_chart`).

## Struttura del workspace

```
backuppo/
├── Cargo.toml              # workspace
├── ROADMAP.md
├── crates/
│   ├── core/               # trait, tipi (inclusi eventi condivisi), errori, config
│   ├── engine/             # esecuzione job, retention, verifica restore
│   ├── sources/            # folder, postgres, mysql, sqlite, docker-volume, command
│   ├── destinations/       # local, sftp, s3, webdav (via opendal)
│   ├── notifiers/          # smtp, telegram, webhook, hub (feature `hub`)
│   ├── cli/                # binario agent: clap, scheduler, logging
│   └── hub/                # (Fase 11) binario hub: API, DB, heartbeat
├── app/                    # (Fase 12) app Flutter
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

- [x] Trait e tipi in `core` (vedi sopra) + `BackupError`
- [x] Parser config con `serde`: `notifiers`, `destinations`, `jobs`
- [x] Validazione: riferimenti a destination/notifier inesistenti, campi mancanti, cron non valido → errori chiari con nome del campo
- [x] Risoluzione segreti da variabili d'ambiente (`password_env`, `token_env`)
- [x] Test unitari su config valide e non valide

**Fatto quando:** `bkpo check --config config.yaml` valida il file e stampa errori leggibili.

## Fase 2 — Primo giro end to end (cartella → locale)

- [ ] `Source` cartella (staging, conteggio file/byte, esclusioni glob)
- [ ] Engine: tar + zstd + cifratura `age` (chiave/passphrase da env)
- [ ] `Destination` locale (opendal fs)
- [ ] Comando `bkpo run --config c.yaml --job <nome>`
- [ ] Logging strutturato con `tracing`
- [ ] Il core non stampa risultati "a mano": ogni esito passa da `JobEvent`
- [ ] Test di integrazione: backup di una cartella temporanea e confronto contenuto

**Fatto quando:** un job cartella → directory locale produce un archivio cifrato e il test lo ripristina identico.

## Fase 3 — Verifica restore (il differenziatore)

- [ ] Comando `bkpo verify --job <nome>`: scarica l'ultimo backup, decifra, decomprime in dir temporanea
- [ ] Controlli per cartelle: numero di file, dimensioni, checksum di un campione
- [ ] Config `verify_restore: never | every | daily | weekly` per job
- [ ] Evento `RestoreVerified` con dettaglio
- [ ] Allarme "backup troppo vecchio" (soglia configurabile)

**Fatto quando:** un backup corrotto di proposito (byte alterato) fa fallire la verifica con messaggio chiaro.

## Fase 4 — Notifiche

- [ ] Notifier Telegram (reqwest + rustls, chiamata `sendMessage`)
- [ ] Notifier SMTP (`lettre`, STARTTLS e TLS implicito)
- [ ] Regole per job: `on_success`, `on_failure`, `on_verify`
- [ ] Comando `bkpo notify-test` per provare i canali
- [ ] Test con mock server per Telegram e SMTP locale

**Fatto quando:** un job fallito manda mail e Telegram, un job riuscito solo Telegram, come da config.

## Fase 5 — Scheduler e retention

- [ ] Modalità daemon: `bkpo daemon` con scheduler cron-like (`tokio-cron-scheduler`)
- [ ] Un job non parte se il precedente è ancora in esecuzione (lock)
- [ ] Retention `daily/weekly/monthly` con cancellazione dei backup scaduti
- [ ] Retention **mai** distruttiva se l'ultimo backup è fallito o non verificato
- [ ] Shutdown pulito su SIGTERM/Ctrl+C

**Fatto quando:** il daemon esegue due job schedulati e applica la retention senza cancellare l'unico backup valido.

## Fase 6 — Destinazioni remote

- [x] SFTP (Hetzner Storage Box come caso di test reale)
- [x] S3-compatibili (B2, Wasabi, MinIO) con test su MinIO in Docker
- [x] WebDAV
- [x] Retry con backoff e upload resumable dove possibile
- [x] Limite di banda opzionale

**Fatto quando:** lo stesso job funziona verso locale, SFTP e S3 cambiando solo la destination.

## Fase 7 — Sorgenti database e Docker

- [x] Postgres (`pg_dump`, anche dentro container via Docker)
- [x] MySQL/MariaDB (`mysqldump`)
- [x] SQLite (backup consistente con `.backup`)
- [x] Volumi Docker (`bollard`)
- [x] Sorgente "comando custom" (stdout) e hook `pre`/`post`
- [x] Verifica restore per DB: caricare il dump in un container temporaneo ed eseguire una query di sanità

**Fatto quando:** un dump Postgres viene ripristinato in un container usa e getta e la query di controllo passa.

## Fase 8 — Report e osservabilità (locale)

- [ ] Report periodico via mail (es. "7 job ok, 1 fallito, ultimo restore test: ieri")
- [ ] Webhook generico (copre Slack, Discord, ntfy)
- [ ] Pagina di stato HTML statica generata a ogni run
- [ ] **Storico esecuzioni strutturato** (SQLite locale o JSON): job, esito, durata, byte, errore, esito verifica restore. Serve poi come base per hub e app
- [ ] Log per esecuzione consultabile da CLI (`bkpo runs`, `bkpo logs <id>`)

**Fatto quando:** arriva il report settimanale con lo stato reale di ogni job e lo storico è interrogabile da CLI.

## Fase 9 — Release agent

- [ ] Build statici Linux (musl), Windows e macOS via CI
- [ ] Immagine Docker minimale
- [ ] Servizio systemd d'esempio e installer Windows (servizio)
- [ ] Documentazione: quickstart, riferimento config, guida al restore manuale
- [ ] Release `v0.1.0` con changelog

## Fase 10 — Dopo l'MVP (da valutare)

- [ ] Motore incrementale/dedup (chunking, oppure integrazione restic)
- [ ] Web UI leggera sopra un'API locale dell'agent (stato, log, avvio manuale)
- [ ] Google Drive / Dropbox / OneDrive (via opendal)
- [ ] Immagini disco / VM (solo se c'è domanda reale)

## Fase 11 — Hub multi-sito (dopo `v0.1.0` stabile)

Prerequisito: agent stabile, storico strutturato (Fase 8).

**Lato agent**
- [ ] `Notifier` di tipo `hub` dietro feature flag `hub` (config: `url`, `token_env`, `heartbeat`)
- [ ] Heartbeat periodico dal daemon
- [ ] Coda locale degli eventi non inviati (file/SQLite) con retry e backoff
- [ ] Un errore di invio all'hub viene loggato ma **non fa mai fallire un job**
- [ ] Test: l'agent compila, gira e passa tutti i test **senza** la feature `hub`

**Lato hub (`crates/hub`, binario separato)**
- [ ] API `/v1`: registrazione eventi, elenco clienti/siti/job/esecuzioni, log
- [ ] Database (SQLite per iniziare, Postgres opzionale)
- [ ] Modello `Cliente → Sito → Job → Esecuzione` e ruoli (admin, sola lettura)
- [ ] Token per agent, creazione e revoca singola
- [ ] Login utenti con JWT a scadenza breve + refresh token
- [ ] **Rilevamento offline**: sito senza heartbeat da X minuti → stato `offline` + notifica
- [ ] Notifiche dell'hub (mail/Telegram/push) su fallimenti, verifiche fallite, siti offline
- [ ] Web UI minimale per test e uso senza app
- [ ] Immagine Docker dell'hub e guida al deploy dietro reverse proxy HTTPS

**Fatto quando:** due agent su macchine diverse mandano stato all'hub, uno viene spento e l'hub lo segna offline e notifica. Con l'hub giù, gli agent continuano a fare backup e rispediscono gli eventi al ritorno.

## Fase 12 — App Flutter

Prerequisito: API hub `/v1` stabile.

- [ ] Progetto Flutter in `app/` (Riverpod, `dio`, `go_router`, `drift` per cache offline, `fl_chart`)
- [ ] Login e gestione connessione all'hub
- [ ] Home con semaforo per sito: ok / warning / errore / offline
- [ ] Dettaglio sito → job → esecuzione con log ed errore
- [ ] Stato ultima verifica restore per ogni job
- [ ] Filtri per cliente e ricerca
- [ ] Notifiche push (Firebase, oppure ntfy/UnifiedPush se self-hosted)
- [ ] Azioni opzionali con permesso dedicato: "esegui ora", "verifica ora"
- [ ] Gestione siti e token agent (creazione, revoca)

**Fatto quando:** dal telefono vedi lo stato di tutti i siti, ricevi la push di un fallimento e apri il log dell'errore.

---

## Regole di qualità (valgono sempre)

- Nessun `unwrap()`/`expect()` fuori dai test: ogni errore va propagato o loggato.
- Mai loggare segreti, token, password o contenuto dei backup.
- Un backup si considera riuscito **solo** dopo che l'upload è confermato e (se attivo) il restore è verificato.
- Ogni nuova sorgente/destinazione/notifier implementa il trait di `core` e ha almeno un test di integrazione.
- Errori sempre con contesto: quale job, quale fase, quale risorsa.
- **L'agent deve compilare, girare e passare tutti i test senza hub.** Le feature dell'hub stanno dietro il feature flag `hub` o in un crate separato; nessun crate dell'agent dipende dall'hub.
- Gli agent inviano all'hub solo metadati, mai contenuto dei backup né segreti.

## Rischi da tenere d'occhio

- Allargare troppo lo scope prima che il nucleo (Fase 2–3) funzioni.
- Iniziare hub o app prima che l'agent sia stabile.
- Complessità di async e trait object: preferisci soluzioni semplici (`Box<dyn Trait>` con `async_trait`).
- Cross-compile Windows/macOS: verifica in CI fin dalle prime fasi, non alla fine.
- Rompere la compatibilità tra agent e hub: versiona l'API (`/v1`) e non cambiare campi esistenti.
- `Cargo.lock` fissa `kem` a `0.3.0-pre.0` (precise pin) per un conflitto reale tra le versioni di `ml-kem` usate da `age` e da `russh`: non lanciare `cargo update` senza `-p` su questa dipendenza, o il build torna a rompersi.
