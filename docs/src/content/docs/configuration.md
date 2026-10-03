---
title: Riferimento configurazione
description: Schema completo del file YAML di Backuppo — destinazioni, sorgenti, job e notifier.
---

La configurazione è YAML. I nomi sotto `destinations`, `notifiers` e `jobs`
sono identificatori scelti dall'utente. `bkpo check --config FILE` controlla
campi obbligatori, riferimenti e schedule prima dell'esecuzione.

## Destinazioni

### Filesystem

```yaml
destinations:
  local:
    type: fs
    root: /var/backups/backuppo
```

### SFTP

```yaml
  remote:
    type: sftp
    host: storage.example.com
    port: 22
    user: backup
    password_env: SFTP_PASSWORD
    key_path: /etc/backuppo/id_ed25519
    key_passphrase_env: SFTP_KEY_PASSPHRASE
    host_key_fingerprint: SHA256:...
    root: /backups
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

Serve almeno uno tra `password_env` e `key_path`. In produzione configura la
fingerprint della host key.

### S3 compatibile

```yaml
  object-storage:
    type: s3
    bucket: backups
    region: eu-central-1
    endpoint: https://s3.example.com
    access_key_id_env: S3_ACCESS_KEY_ID
    secret_access_key_env: S3_SECRET_ACCESS_KEY
    root: site-a
    virtual_host_style: false
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

`endpoint` è opzionale per AWS. Il path style è il default ed è normalmente
quello richiesto da MinIO e dagli storage self-hosted.

### WebDAV

```yaml
  webdav:
    type: webdav
    url: https://dav.example.com/backups
    user: backup
    password_env: WEBDAV_PASSWORD
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

### Google Drive, Dropbox e OneDrive

I tre backend usano OAuth. Il token di accesso è obbligatorio; per un daemon
continuativo configura anche refresh token, client ID e client secret:

```yaml
  drive:
    type: google_drive            # oppure dropbox | one_drive
    root: /backuppo
    access_token_env: DRIVE_ACCESS_TOKEN
    refresh_token_env: DRIVE_REFRESH_TOKEN
    client_id: application-client-id
    client_secret_env: DRIVE_CLIENT_SECRET
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

### Restic incrementale

Restic conserva chunk deduplicati e snapshot incrementali. Le release Backuppo
includono una versione verificata accanto a `bkpo`; `binary` può indicarne una
alternativa. `initialize` crea il repository al primo backup.

```yaml
  incremental:
    type: restic
    repository: s3:https://s3.example.com/backups/restic
    password_env: RESTIC_PASSWORD
    initialize: true
    append_only: true
    environment:
      AWS_ACCESS_KEY_ID: S3_BACKUP_ACCESS_KEY
      AWS_SECRET_ACCESS_KEY: S3_BACKUP_SECRET_KEY
    maintenance_environment:
      AWS_ACCESS_KEY_ID: S3_ADMIN_ACCESS_KEY
      AWS_SECRET_ACCESS_KEY: S3_ADMIN_SECRET_KEY
    object_lock:
      bucket: backups
      region: eu-central-1
      endpoint: https://s3.example.com
      access_key_id_env: S3_LOCK_CHECK_ACCESS_KEY
      secret_access_key_env: S3_LOCK_CHECK_SECRET_KEY
      expected_mode: compliance
      minimum_retention_days: 30
```

I valori delle mappe sono nomi di variabili, mai segreti letterali. Con
`append_only: true` l'agent non esegue `forget` o `prune`. La manutenzione
manuale usa esclusivamente `maintenance_environment`:

```bash
bkpo maintain --config /etc/backuppo/config.yaml --job home-documents
bkpo storage-check --config /etc/backuppo/config.yaml --job home-documents
```

`storage-check` firma una richiesta S3 `GetObjectLockConfiguration` e fallisce
se Object Lock, modalità o durata minima non corrispondono alla policy attesa.

## Sorgenti

```yaml
source:
  type: folder
  path: /srv/data
  exclude: ["*.tmp", "cache/"]
```

Database PostgreSQL e MySQL/MariaDB usano rispettivamente `pg_dump` e
`mysqldump`. Se `container` è presente, il comando viene eseguito dentro quel
container; altrimenti il client deve essere installato sull'host.

```yaml
source:
  type: postgres                 # oppure mysql
  host: localhost
  port: 5432                     # MySQL: 3306
  user: postgres
  password_env: DATABASE_PASSWORD
  database: app
  container: app-postgres        # opzionale
```

Le altre sorgenti sono:

```yaml
source: { type: sqlite, path: /srv/app.sqlite }
source: { type: docker_volume, volume: app_data }
source:
  type: command
  command: /usr/local/bin/export-data
  args: ["--format", "json"]
  output_filename: export.json
source: { type: disk_image, path: /dev/disk/by-id/example, output_filename: disk.img }
source: { type: libvirt_vm, name: app-vm }
source: { type: proxmox_vm, vmid: 100, mode: snapshot }
```

`disk_image` copia byte per byte un file o device. `libvirt_vm` salva XML e
dischi elencati da `virsh`; la VM deve essere spenta per evitare immagini
inconsistenti. Hook `pre` e `post` possono gestire l'arresto e il riavvio.

`proxmox_vm` esegue `vzdump <vmid> --dumpdir <staging>`: l'agent deve girare
sul nodo Proxmox stesso. `vzdump` rileva da solo se il vmid è una VM QEMU o
un container LXC. `mode` accetta `snapshot` (default, non ferma la VM),
`suspend` o `stop`; `vzdump_binary` permette di indicare un percorso non
standard.

## Job

```yaml
jobs:
  documents:
    source: { type: folder, path: /srv/documents }
    destination: local
    engine: restic                # default del wizard; archive resta compatibile
    compression: zstd            # zstd | none; default zstd
    encryption:
      type: age                  # age | none
      passphrase_env: BACKUP_PASSPHRASE
    schedule: "0 3 * * *"       # minuto ora giorno mese giorno-settimana
    verify_restore: daily        # never | every | daily | weekly
    max_backup_age_hours: 26
    retention:
      daily: 7
      weekly: 4
      monthly: 6
    pre: [/usr/local/bin/before-backup]
    post: [/usr/local/bin/after-backup]
    notify:
      on_success: [ops-webhook]
      on_failure: [ops-email, ops-telegram]
      on_verify: [ops-email]
```

Con il motore `archive`, `age` accetta esattamente uno tra `passphrase_env` e
`key_env`. `key_env` deve contenere una identità privata X25519
`AGE-SECRET-KEY-...`; Backuppo deriva il recipient pubblico per cifrare e usa
la stessa identità per il restore.

Con `engine: restic`, `destination` deve riferirsi a una destination
`type: restic`; compressione e cifratura sono gestite da Restic.

Un hook `pre` fallito interrompe il job. Un hook `post` fallito viene
registrato, ma non invalida un archivio già caricato e verificato.

## Notifier

```yaml
notifiers:
  ops-telegram:
    type: telegram
    token_env: TELEGRAM_BOT_TOKEN
    chat_id: "123456"
  ops-email:
    type: smtp
    host: smtp.example.com
    port: 587
    user: backups@example.com
    password_env: SMTP_PASSWORD
    from: backups@example.com
    to: [ops@example.com]
  ops-webhook:
    type: webhook
    url: https://hooks.example.com/backuppo
```

Il webhook invia JSON con `event`, `job`, `text`, `content` e `message`, più i
metadati dell'archivio quando disponibili. Adatto a Slack e Discord; **per ntfy
usa il tipo `ntfy`** (sotto): ntfy tratta il corpo di una `POST /topic` come
testo del messaggio, non come JSON.

### ntfy (push sul telefono)

```yaml
notifiers:
  ops-ntfy:
    type: ntfy
    url: https://ntfy.sh          # o la tua istanza self-hosted
    topic: backuppo-a1b2c3d4e5    # scegli un nome non indovinabile
    token_env: NTFY_TOKEN         # opzionale: solo per topic protetti
```

Pubblica su `<url>/<topic>` con titolo, priorità e tag: i fallimenti hanno
priorità alta (4), successi e verifiche normale (3), i report bassa (2). Su
un topic pubblico chiunque conosca il nome può leggere e scrivere: usa un
nome lungo e casuale, oppure un'istanza self-hosted con controllo accessi e
`token_env`. I messaggi contengono nome del job ed errore, mai il contenuto
dei backup. Per riceverli installa l'app ntfy (o l'app Backuppo, che apre
l'iscrizione al topic) e iscriviti allo stesso topic.

### Hub multi-sito (opt-in)

La feature Cargo `hub` è inclusa nei binari ufficiali. Per un agent minimale
completamente stand-alone si può compilare con `--no-default-features`: in
quel caso il tipo `hub` non esiste nemmeno nel parsing della configurazione.

```yaml
notifiers:
  sede-centrale:
    type: hub
    url: https://hub.example.com
    token_env: HUB_TOKEN
    heartbeat_seconds: 60        # default: 60
    queue_path: /var/lib/backuppo/hub-queue.sqlite  # default: ./backuppo-hub-queue.sqlite
    remote_commands: false       # default: false (vedi sotto)
    command_poll_seconds: 15     # default: 15, usato solo con remote_commands
    policy_public_key: BASE64_ED25519_PUBLIC_KEY
    policy_site_id: 1
    policy_state_path: /var/lib/backuppo/policy-state.json
    policy_poll_seconds: 300
```

L'agent manda all'hub solo metadati (esito, byte, checksum, errore),
mai contenuto dei backup né segreti. Se configurato, il daemon (`bkpo
daemon`) manda un heartbeat all'hub ogni `heartbeat_seconds` e, allo stesso
intervallo, ritenta la consegna degli eventi rimasti nella coda locale
(`queue_path`) con backoff esponenziale (da 30s fino a un tetto di un'ora).
Un hub irraggiungibile non fa mai fallire un job né arresta il daemon:
l'errore viene solo loggato.

**Comandi remoti (opt-in).** Con `remote_commands: true` il daemon chiede
all'hub ogni `command_poll_seconds` se c'è un "esegui ora" o "verifica ora"
richiesto da un utente `admin`/`operator` (dall'app o dall'API dell'hub), lo
esegue e riporta l'esito. Resta una connessione solo in uscita: l'hub non
apre mai connessioni verso l'agent. Sicurezza: il comando può solo scegliere
tra backup e verifica su un job **già presente in questa config** (mai un
comando arbitrario), rispetta il lock per job (se il job è già in
esecuzione la richiesta fallisce) ed è disattivato di default.

Con `policy_public_key` l'agent scarica solo policy firmate per il proprio
sito. Sequenze precedenti o riutilizzate con contenuto diverso vengono
rifiutate. Una policy `block` non conforme sospende scheduler, avvii dalla UI
e comandi remoti; `audit` registra invece le violazioni senza fermare i job.

## Storico, pagina di stato e report

```yaml
observability:
  history_path: /var/lib/backuppo/history.sqlite
  status_page: /var/lib/backuppo/status.html

reports:
  - schedule: "0 8 * * 1"
    days: 7
    notifiers: [ops-email]
```

Senza `observability` l'agent continua a funzionare, ma non scrive storico o
pagina HTML. I report richiedono lo storico e vengono eseguiti dal daemon.

## API e Web UI locale

```yaml
api:
  bind: 127.0.0.1:8787
```

Il daemon avvia automaticamente l'interfaccia su quell'indirizzo. In
alternativa usa `bkpo serve --config config.yaml`. L'API espone
`GET /api/v1/status`, `GET /api/v1/runs`, `GET /api/v1/runs/{id}` e
`POST /api/v1/jobs/{nome}/run`. Per evitare esposizioni accidentali il bind
accetta solo indirizzi IP loopback. Nei container è possibile impostare
`allow_remote: true`, ma la porta va pubblicata su loopback o protetta da
autenticazione esterna. La richiesta `POST` richiede anche
l'header `X-Backuppo-UI: 1`, usato dalla UI per impedire trigger cross-site.

La UI corrente usa azioni asincrone tramite
`POST /api/v1/jobs/{nome}/actions`, supporta anche verifica restore, test dei
notifier e gli endpoint `/api/v1/config/*` per validare, salvare e applicare
la configurazione. Le mutazioni richiedono il token CSRF restituito da
`GET /api/v1/session`. Vedi [wizard e più job](../wizard-job/) per il flusso completo.

## Log

I log vanno su **stderr** (stdout è riservato ai risultati dei comandi) e
sono senza colori quando stderr non è un terminale, quindi file di log e
journald restano leggibili. Senza `RUST_LOG`:

- `bkpo daemon` e `bkpo serve` loggano a livello `info` per i crate di
  Backuppo e `warn` per le dipendenze;
- i comandi one-shot (`run`, `verify`, `check`, ...) mostrano solo `warn` ed
  errori.

Per cambiare il livello imposta `RUST_LOG`, ad esempio `RUST_LOG=debug` o
`RUST_LOG=info,backuppo_engine=debug`. Attenzione ai nomi: il target del
binario è `bkpo`, non `backuppo`.
