# Web UI e client desktop

## Agent locale

Configura `observability` e `api`, poi avvia il daemon normalmente:

```yaml
observability:
  history_path: /var/lib/backuppo/history.sqlite
api:
  bind: 127.0.0.1:8787
```

Apri `http://127.0.0.1:8787`. Il bind dell'agent resta obbligatoriamente
loopback. La UI permette di avviare backup e verifiche senza mantenere aperta
la richiesta HTTP: ogni azione restituisce un ID e viene seguita fino
all'esito.

La configurazione viene prima validata dal parser Rust e poi salvata. Il
salvataggio rifiuta modifiche concorrenti, crea `config.yaml.bak`, sostituisce
il file atomicamente e ne conserva i permessi. I valori delle variabili
`*_env` non vengono letti o memorizzati dalla UI.

Il pulsante **Applica** ricostruisce scheduler, lock e task dell'hub senza
interrompere i job già in corso. Una modifica a `api.bind` richiede il riavvio
perché il listener attivo non viene spostato.

## Hub

La UI è disponibile sulla radice dell'hub. In produzione pubblicala sempre
tramite il reverse proxy HTTPS descritto in [hub-deploy.md](hub-deploy.md): le
sessioni browser usano cookie `HttpOnly`, `Secure`, `SameSite=Strict` e token
CSRF. Gli endpoint bearer `/v1` rimangono disponibili per agent e client
esistenti.

I ruoli conservano la semantica dell'API: `read_only` consulta, `operator`
può anche inviare run/verify e `admin` gestisce clienti, siti, utenti e token.

## PWA e Tauri

Il manifest permette di installare la UI dal browser. Il service worker mette
in cache soltanto l'app shell: API, credenziali e mutazioni restano sempre
online.

Il wrapper in `apps/desktop` apre la medesima UI servita da un agent o hub e
non incorpora il daemon. HTTP è accettato soltanto su loopback; un hub remoto
deve usare HTTPS.

## Sviluppo frontend

```sh
cd apps/web
npm ci
npm run build
```

Il bundle in `apps/web/dist` viene incorporato nei due binari. Docker e CI
eseguono automaticamente questi passaggi.
