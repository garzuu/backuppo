# ROADMAP — Backuppo: backup open source (stile Iperius) in Rust

Nome progetto: **Backuppo** · crate: `backuppo` · binario (comando): `bkpo`

## Come usare questo file con Claude Code

- Lavora **una fase alla volta**, nell'ordine. Non anticipare fasi successive.
- Ogni task ha un criterio di completamento: spuntalo solo se i test passano.
- Prima di scrivere codice in una fase, leggi le "Decisioni prese" qui sotto: non rimetterle in discussione senza chiedere.
- Dopo ogni fase: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, poi commit.
- Se una decisione cambia, aggiorna questo file.

## Stato

- Fasi 0–10: agent completo e rilasciato (`v0.1.0`), incluse le evoluzioni di Fase 10 (motore dedup, web UI locale, storage cloud aggiuntivi, immagini disco/VM).
- Fasi 11–12: hub multi-sito e app Flutter completati.
- **Prossimo passo: Fase 13 (sito e documentazione).** Poi lancio (14) e le evoluzioni successive.

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
- Modello dati hub: `Organizzazione/Cliente → Sito → Job → Esecuzione`, con ruoli (admin, operator, sola lettura). L'`operator` può chiedere "esegui ora"/"verifica ora" agli agent che hanno attivato `remote_commands`.
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
│   └── hub/                # binario hub: API /v1, DB, heartbeat, offline detection
└── examples/config.yaml, examples/hub-config.yaml
```

Regola: `core` non dipende da nessun altro crate del workspace; tutti gli altri dipendono da `core`.

**Repo**: agent e hub restano in **questo** repository (stesso workspace Cargo), così l'hub riusa direttamente i tipi condivisi di `core` senza doverli pubblicare o pinnare come dipendenza esterna. L'**app Flutter** (Fase 12) vive invece in un **repository separato**: toolchain e pipeline di build completamente diverse, nessun beneficio dallo stare nello stesso workspace Cargo.

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

- [x] `Source` cartella (staging, conteggio file/byte, esclusioni glob)
- [x] Engine: tar + zstd + cifratura `age` (chiave/passphrase da env)
- [x] `Destination` locale (opendal fs)
- [x] Comando `bkpo run --config c.yaml --job <nome>`
- [x] Logging strutturato con `tracing`
- [x] Il core non stampa risultati "a mano": ogni esito passa da `JobEvent`
- [x] Test di integrazione: backup di una cartella temporanea e confronto contenuto

**Fatto quando:** un job cartella → directory locale produce un archivio cifrato e il test lo ripristina identico.

## Fase 3 — Verifica restore (il differenziatore)

- [x] Comando `bkpo verify --job <nome>`: scarica l'ultimo backup, decifra, decomprime in dir temporanea
- [x] Controlli per cartelle: numero di file, dimensioni, checksum di un campione
- [x] Config `verify_restore: never | every | daily | weekly` per job
- [x] Evento `RestoreVerified` con dettaglio
- [x] Allarme "backup troppo vecchio" (soglia configurabile)

**Fatto quando:** un backup corrotto di proposito (byte alterato) fa fallire la verifica con messaggio chiaro.

## Fase 4 — Notifiche

- [x] Notifier Telegram (reqwest + rustls, chiamata `sendMessage`)
- [x] Notifier SMTP (`lettre`, STARTTLS e TLS implicito)
- [x] Regole per job: `on_success`, `on_failure`, `on_verify`
- [x] Comando `bkpo notify-test` per provare i canali
- [x] Test con mock server per Telegram e SMTP locale

**Fatto quando:** un job fallito manda mail e Telegram, un job riuscito solo Telegram, come da config.

## Fase 5 — Scheduler e retention

- [x] Modalità daemon: `bkpo daemon` con scheduler cron-like (`tokio-cron-scheduler`)
- [x] Un job non parte se il precedente è ancora in esecuzione (lock)
- [x] Retention `daily/weekly/monthly` con cancellazione dei backup scaduti
- [x] Retention **mai** distruttiva se l'ultimo backup è fallito o non verificato
- [x] Shutdown pulito su SIGTERM/Ctrl+C

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

- [x] Report periodico via mail (es. "7 job ok, 1 fallito, ultimo restore test: ieri")
- [x] Webhook generico (Slack, Discord). ntfy ha un notifier dedicato (`type: ntfy`, Fase 12): il webhook JSON non è adatto a `POST /topic`
- [x] Pagina di stato HTML statica generata a ogni run
- [x] **Storico esecuzioni strutturato** (SQLite locale o JSON): job, esito, durata, byte, errore, esito verifica restore. Serve poi come base per hub e app
- [x] Log per esecuzione consultabile da CLI (`bkpo runs`, `bkpo logs <id>`)

**Fatto quando:** arriva il report settimanale con lo stato reale di ogni job e lo storico è interrogabile da CLI.

## Fase 9 — Release agent

- [x] Build statici Linux (musl), Windows e macOS via CI
- [x] Immagine Docker minimale
- [x] Servizio systemd d'esempio e installer Windows (servizio)
- [x] Documentazione: quickstart, riferimento config, guida al restore manuale
- [x] Release `v0.1.0` con changelog

## Fase 10 — Dopo l'MVP (da valutare)

- [x] Motore incrementale/dedup (integrazione Restic)
- [x] Web UI leggera sopra un'API locale dell'agent (stato, log, avvio manuale)
- [x] Google Drive / Dropbox / OneDrive (via opendal)
- [x] Immagini disco / VM (file/device e VM libvirt spente)

## Fase 11 — Hub multi-sito (dopo `v0.1.0` stabile)

Prerequisito: agent stabile, storico strutturato (Fase 8).

**Lato agent**
- [x] `Notifier` di tipo `hub` dietro feature flag `hub` (config: `url`, `token_env`, `heartbeat`)
- [x] Heartbeat periodico dal daemon
- [x] Coda locale degli eventi non inviati (file/SQLite) con retry e backoff
- [x] Un errore di invio all'hub viene loggato ma **non fa mai fallire un job**
- [x] Test: l'agent compila, gira e passa tutti i test **senza** la feature `hub`

**Lato hub (`crates/hub`, binario separato)**
- [x] API `/v1`: registrazione eventi, elenco clienti/siti/job/esecuzioni, log
- [x] Database (SQLite per iniziare, Postgres opzionale — non ancora necessario)
- [x] Modello `Cliente → Sito → Job → Esecuzione` e ruoli (admin, sola lettura)
- [x] Token per agent, creazione e revoca singola
- [x] Login utenti con JWT a scadenza breve + refresh token
- [x] **Rilevamento offline**: sito senza heartbeat da X minuti → stato `offline` + notifica
- [x] Notifiche dell'hub (mail/Telegram/webhook/ntfy) su fallimenti, verifiche fallite, siti offline
- [x] Web UI minimale per test e uso senza app
- [x] Immagine Docker dell'hub e guida al deploy dietro reverse proxy HTTPS

Nota: il modello `Job` non ha una tabella propria — è il valore distinto
del campo `job` sulle esecuzioni ricevute (`GET /v1/sites/{id}/jobs`),
così non serve pre-registrare i nomi dei job lato hub.

Verifica (21/09/2026): due daemon `bkpo --features hub` + hub in locale (stessa macchina, non due macchine): un agent spento con `kill -9` viene segnato offline in ~100 s con notifica; con l'hub spento un backup riesce e l'evento in coda viene consegnato al ritorno dell'hub (backoff 60 s → max 1 h). Da ripetere su due macchine reali.

**Fatto quando:** due agent su macchine diverse mandano stato all'hub, uno viene spento e l'hub lo segna offline e notifica. Con l'hub giù, gli agent continuano a fare backup e rispediscono gli eventi al ritorno.

## Fase 12 — App Flutter

Prerequisito: API hub `/v1` stabile. **Repo separato** da `backuppo` (vedi "Struttura del workspace").

- [x] Progetto Flutter in un repository dedicato (`~/Code/backuppo-app`; Riverpod, `dio`, `go_router`, `drift` per cache offline, `fl_chart`)
- [x] Login e gestione connessione all'hub (URL, refresh automatico del token, sessione nello storage sicuro)
- [x] Home con semaforo per sito: ok / warning / errore / offline
- [x] Dettaglio sito → job → esecuzione con log ed errore
- [x] Stato ultima verifica restore per ogni job
- [x] Filtri per cliente e ricerca
- [x] Notifiche push via **ntfy** (scelta: self-hostabile, nessun account Firebase/Apple da gestire): notifier `type: ntfy` (agent e hub, priorità alta sui fallimenti) + schermata "Notifiche" nell'app con link di iscrizione e invio di prova. Verificato solo con server mock: da provare con un server ntfy reale e l'app ntfy su un telefono (deep link `ntfy://` non verificato su dispositivo)
- [x] Azioni opzionali con permesso dedicato: "esegui ora", "verifica ora". Canale di comando **a polling in uscita** (l'agent ritira dall'hub, l'hub non si connette mai all'agent): opt-in `remote_commands` sull'agent, ruolo hub `operator`, comandi che scadono dopo 10 min, solo job presenti nella config dell'agent. Verificato end-to-end (hub + agent + app macOS)
- [x] Gestione siti e token agent (creazione; revoca per ID — l'hub non ha ancora un endpoint per elencare i token)

Nota: il semaforo è calcolato dall'app sugli eventi (l'hub non espone uno stato aggregato per sito oltre a online/offline). `POST /v1/sites/{id}/tokens` ora restituisce anche `token_id` (campo aggiunto, compatibile con `/v1`).

**Fatto quando:** dal telefono vedi lo stato di tutti i siti, ricevi la push di un fallimento e apri il log dell'errore.

## Fase 13 — Sito e documentazione (DA INIZIARE)

Obiettivo: chi arriva sul sito capisce in 30 secondi cosa fa Backuppo e come si installa.

- [ ] Scelta dominio (`backuppo.dev` o simile, da verificare) e hosting statico (GitHub Pages o Cloudflare Pages)
- [x] Generatore di sito statico con docs integrate (Astro Starlight in `docs/`, contenuti in `docs/src/content/docs/`); versionati col codice. Archivi di release e link nel repo puntano ai singoli `.md`, non all'intero progetto Astro
- [ ] **Landing page**: cos'è, il problema ("hai mai provato a fare il restore?"), il differenziatore (restore verificato), installazione in una riga, mascotte
- [ ] **Demo** di 1–2 minuti (GIF o asciinema): backup → corruzione di un byte → `bkpo verify` che fallisce con messaggio chiaro
- [ ] Tabella di confronto onesta con Iperius, restic, borg, kopia (cosa fa meglio, cosa non fa)
- [ ] Documentazione: quickstart, riferimento config completo, guida al restore manuale, sorgenti/destinazioni/notifiche, hub e app
- [ ] Pagina Download con link alle release e istruzioni per Linux, Windows, macOS, Docker
- [ ] Changelog pubblico e pagina Roadmap (derivata da questo file)
- [ ] Mascotte (ippopotamo), logo e favicon; immagine Open Graph per le anteprime social
- [ ] SEO di base (titoli, meta, sitemap) e analytics rispettosi della privacy, oppure nessuna analytics
- [ ] CI che pubblica il sito a ogni merge su `main` e controlla i link rotti

**Fatto quando:** il sito è online sul dominio scelto, con landing, demo e docs complete, e una persona che non conosce il progetto riesce a installarlo e a fare un primo backup seguendo solo il quickstart.

## Fase 14 — Distribuzione e lancio

- [ ] Pacchetti: Homebrew, winget, AUR, `.deb`/`.rpm`
- [ ] Immagine Docker su registry pubblico (GHCR e Docker Hub)
- [ ] Script di installazione in una riga con verifica checksum e firma delle release
- [ ] `README.md` del repo allineato al sito (badge, demo, quickstart)
- [ ] Template per issue e discussioni, `CONTRIBUTING.md`, `SECURITY.md`
- [ ] Lancio su r/selfhosted, r/homelab e Hacker News (Show HN), con il demo del restore verificato
- [ ] Raccolta feedback e priorità della prossima fase basate sulle richieste reali

**Fatto quando:** l'installazione funziona con i gestori di pacchetti principali e il progetto è stato presentato almeno su due community.

## Fase 15 — Protezione da ransomware

- [ ] Destinazioni append-only / object lock (S3 immutabile, `chattr +a` o equivalenti)
- [ ] Credenziali di upload con permessi di sola scrittura, senza diritto di cancellazione
- [ ] Retention gestita lato destinazione o da un ruolo separato, non dall'agent
- [ ] Rotazione delle chiavi di cifratura e backup delle chiavi (guida + comando `bkpo keys`)
- [ ] Documentazione "modello di minaccia": cosa succede se il server sorgente viene compromesso

**Fatto quando:** con un agent compromesso non è possibile cancellare né sovrascrivere i backup già caricati.

## Fase 16 — Restore assistito

- [ ] `bkpo restore` interattivo: scegli job, data e destinazione
- [ ] Sfogliare un backup e recuperare singoli file o cartelle
- [ ] Restore di database direttamente in un container o server di destinazione
- [ ] Modalità `--dry-run` che mostra cosa verrebbe ripristinato

**Fatto quando:** un utente ripristina un singolo file da un backup di tre settimane fa senza leggere la documentazione del formato.

## Fase 17 — Interfaccia locale

- [ ] Web UI servita dal daemon con editor di config (job, destinazioni, notifiche) che scrive lo stesso YAML
- [ ] Vista stato, log ed esecuzioni; avvio manuale di job e verifica
- [ ] Opzionale: finestra desktop con Tauri e icona nella tray per gli utenti Windows

**Fatto quando:** un utente Windows configura ed esegue un job completo senza toccare il file YAML.

## Fase 18 — Integrazioni

- [ ] Endpoint metriche Prometheus
- [ ] Integrazione Home Assistant
- [ ] Server MCP per interrogare lo stato dei backup da Claude ("quali job sono falliti questa settimana?"), in sola lettura
- [ ] Valutare modello a pagamento per l'hub gestito, lasciando l'agent open source

---

## Fase 13 — UI web-first

- [x] SPA React/TypeScript condivisa tra agent e hub, incorporata nei binari
- [x] Dashboard agent, storico/log e run/verify asincroni
- [x] Validazione, salvataggio atomico e reload esplicito della config locale
- [x] Dashboard hub, comandi remoti e amministrazione utenti/token
- [x] PWA con cache limitata all'app shell
- [x] Sessione browser hub con cookie HttpOnly e protezione CSRF
- [x] Wrapper Tauri opzionale senza sidecar agent
- [ ] Form guidati completi per ogni variante di source/destination/notifier
- [ ] Test end-to-end Playwright su viewport desktop e mobile

L'app Flutter viene mantenuta solo durante la transizione e potrà essere
archiviata dopo la parità mobile e la verifica ntfy della PWA.

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
