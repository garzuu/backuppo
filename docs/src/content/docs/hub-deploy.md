---
title: Deploy dell'hub multi-sito
description: Come compilare, configurare e mettere in produzione backuppo-hub.
---

L'hub (`backuppo-hub`) è un binario separato dall'agent, pensato per girare
su una sola macchina di controllo e ricevere solo metadati dagli agent
(mai contenuto dei backup né segreti). Parla solo HTTP in chiaro: **va
sempre messo dietro un reverse proxy che termina TLS**.

## Build

```sh
cargo build --locked --release --package backuppo-hub
```

Oppure via Docker:

```sh
docker build -f Dockerfile.hub -t backuppo-hub:local .
```

## Config

Copiare `examples/hub-config.yaml` e adattarlo. Campi principali:

- `bind`: indirizzo di ascolto (default `0.0.0.0:8080`; dietro reverse
  proxy va bene, non esporre direttamente su Internet).
- `database_path`: file SQLite con clienti, siti, token ed esecuzioni.
- `jwt_secret_env`: variabile d'ambiente con il segreto per firmare i JWT
  di accesso. Generarlo con `openssl rand -hex 32` e non inserirlo mai nel
  file di config.
- `offline_after_minutes`: soglia oltre la quale un sito senza heartbeat
  viene marcato offline e notificato.
- `notifiers` / `notify_on_offline` / `notify_on_failure`: stesso formato
  dei notifier dell'agent (telegram/smtp/webhook/ntfy), riusati per gli alert
  dell'hub. `notify_on_offline` scatta quando un sito passa offline,
  `notify_on_failure` non appena un agent riporta un job o una verifica
  restore falliti su `/v1/events` (indipendentemente dai notifier che
  l'agent stesso ha già usato in locale).

## Primo utente amministratore

Non esiste un endpoint di registrazione pubblico. Il primo utente (e
qualunque altro) si crea da riga di comando:

```sh
export HUB_ADMIN_PASSWORD='...'
backuppo-hub create-user \
  --config /etc/backuppo-hub/config.yaml \
  --username admin \
  --password-env HUB_ADMIN_PASSWORD \
  --role admin
```

`--role` accetta `admin` (può creare clienti/siti/token e gestire tutto),
`operator` (sola lettura + può chiedere "esegui ora"/"verifica ora" agli
agent, vedi sotto) o `read_only` (solo lettura di stato ed esecuzioni).

## Avvio

```sh
export HUB_JWT_SECRET='...'
backuppo-hub serve --config /etc/backuppo-hub/config.yaml
```

La Web UI è servita su `/`, l'API su `/v1/...`. Gli agent si autenticano
sui soli endpoint `/v1/events` e `/v1/heartbeat` con il token generato
alla creazione del sito (mostrato una sola volta); utenti umani usano
`/v1/auth/login` per ottenere un access token JWT di breve durata e un
refresh token rotante.

### Endpoint principali

| Metodo | Percorso | Auth | Note |
| --- | --- | --- | --- |
| POST | `/v1/auth/login` | — | credenziali → coppia access/refresh token |
| POST | `/v1/auth/refresh` | — | refresh token → nuova coppia (rotazione) |
| POST | `/v1/events` | token agent | ingest evento (successo/fallimento/verifica/report) |
| POST | `/v1/heartbeat` | token agent | segna il sito online |
| GET/POST | `/v1/customers` | utente / admin | elenco clienti / creazione |
| GET/POST | `/v1/sites` | utente / admin | elenco siti / creazione (la creazione ritorna il token agent) |
| GET | `/v1/sites/{id}/executions` | utente | storico esecuzioni del sito |
| GET | `/v1/sites/{id}/jobs` | utente | nomi dei job distinti riportati dal sito |
| GET/POST | `/v1/sites/{id}/commands` | utente / admin, operator | ultimi 50 comandi del sito / richiesta `{"kind": "run"\|"verify", "job": "..."}` (409 se già in corso) |
| GET | `/v1/commands/pending` | token agent | l'agent ritira i suoi comandi in attesa (ognuno una sola volta) |
| POST | `/v1/commands/{id}/result` | token agent | l'agent riporta l'esito `{"ok": bool, "detail": "..."}` |
| POST | `/v1/sites/{id}/tokens` | admin | genera un nuovo token agent per il sito (ritorna anche `token_id`) |
| DELETE | `/v1/sites/{id}/tokens/{token_id}` | admin | revoca un token agent |

## Comandi remoti ("esegui ora", "verifica ora")

L'hub non si connette mai agli agent: una richiesta resta in coda sull'hub
finché l'agent, che sta dietro NAT/firewall, la ritira col suo poll in
uscita (`GET /v1/commands/pending`, ogni `command_poll_seconds`). Poi
esegue l'azione e riporta l'esito. Stati: `pending` → `delivered` →
`done` | `failed`, oppure `expired`.

- **Opt-in sull'agent:** senza `remote_commands: true` nel notifier `hub`
  l'agent non ritira nulla, e la richiesta scade.
- **Scadenza:** una richiesta non ritirata entro 10 minuti diventa
  `expired` e non viene mai consegnata: un backup non parte ore dopo che
  qualcuno l'ha chiesto perché l'agent era offline.
- **Solo azioni note:** `run` (backup + retention) e `verify` (verifica
  restore), solo su job già presenti nella config dell'agent. Un job
  sconosciuto o già in esecuzione fallisce con un messaggio chiaro.
- **Niente doppi avvii:** l'hub rifiuta (409) una richiesta identica a una
  ancora in corso.
- **Permessi:** solo `admin` e `operator`; `read_only` riceve 403.

## systemd (bare metal)

```sh
sudo useradd --system --home /var/lib/backuppo-hub --shell /usr/sbin/nologin backuppo-hub
sudo install -d -o backuppo-hub -g backuppo-hub /var/lib/backuppo-hub /etc/backuppo-hub
sudo install -m 0644 deploy/systemd/backuppo-hub.service /etc/systemd/system/
sudo install -m 0600 -o backuppo-hub -g backuppo-hub deploy/systemd/backuppo-hub.env.example /etc/backuppo-hub/backuppo-hub.env
# modificare /etc/backuppo-hub/backuppo-hub.env e /etc/backuppo-hub/config.yaml
sudo systemctl daemon-reload
sudo systemctl enable --now backuppo-hub
```

## Docker

```sh
docker run -d --name backuppo-hub \
  -v /srv/backuppo-hub/config.yaml:/etc/backuppo-hub/config.yaml:ro \
  -v backuppo-hub-data:/var/lib/backuppo-hub \
  -e HUB_JWT_SECRET='...' \
  -p 127.0.0.1:8080:8080 \
  backuppo-hub:local
```

Pubblicare la porta solo su loopback (come sopra) e lasciare che sia il
reverse proxy a esporla su Internet.

## Reverse proxy con TLS

### Caddy

```
hub.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Caddy ottiene e rinnova il certificato automaticamente.

### nginx

```
server {
    listen 443 ssl;
    server_name hub.example.com;

    ssl_certificate     /etc/letsencrypt/live/hub.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/hub.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Backup dell'hub stesso

`database_path` contiene clienti, siti, hash dei token e lo storico delle
esecuzioni riportate dagli agent (mai contenuto dei backup). Include
questo file in un job dell'agent stesso, o quantomeno in un backup
periodico separato: se si perde, si perdono solo storico e registrazioni,
mai i backup effettivi (che restano dove li hanno messi gli agent).

## Sicurezza

- Un token agent compromesso permette solo di inviare eventi/heartbeat
  fittizi per quel sito, mai di leggere dati di altri siti né di accedere
  alla Web UI: va comunque revocato (`DELETE
  /v1/sites/{id}/tokens/{token_id}`) e rigenerato se sospetto.
- Gli access token JWT scadono in pochi minuti (`access_token_minutes`);
  i refresh token ruotano a ogni uso (`POST /v1/auth/refresh` invalida
  quello consumato e ne restituisce uno nuovo).
- Un ruolo `read_only` non può creare clienti, siti o token: solo
  consultare stato ed esecuzioni. Un `operator` può in più avviare backup e
  verifiche sugli agent che hanno attivato i comandi remoti, ma non gestire
  clienti, siti o token.
- Chi può creare comandi può far eseguire backup e verifiche (non comandi
  arbitrari) sugli agent con `remote_commands: true`: assegna il ruolo
  `operator` con la stessa cautela di un accesso alla macchina.
