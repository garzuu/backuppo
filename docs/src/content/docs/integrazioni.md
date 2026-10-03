---
title: Integrazioni
description: Metriche Prometheus, Home Assistant e server MCP per interrogare lo stato dei backup da strumenti esterni.
---

## Prometheus

L'Agent espone `GET /metrics` in formato di testo Prometheus, sulla stessa
porta della Web UI (`api.bind`, default `127.0.0.1:8787`). Richiede
`observability.history_path` configurato, come la status page: restituisce
`503` altrimenti.

```yaml
# prometheus.yml
scrape_configs:
  - job_name: backuppo
    metrics_path: /metrics
    static_configs:
      - targets: ['127.0.0.1:8787']
```

Per ogni job, se ha almeno un'esecuzione nello storico:

| Metrica | Descrizione |
| --- | --- |
| `backuppo_last_run_timestamp_seconds{job="..."}` | Unix timestamp dell'inizio dell'ultima esecuzione |
| `backuppo_last_run_success{job="..."}` | `1` se riuscita, `0` altrimenti |
| `backuppo_last_run_duration_ms{job="..."}` | Durata in millisecondi |
| `backuppo_last_run_bytes{job="..."}` | Byte trasferiti |
| `backuppo_last_verify_timestamp_seconds{job="..."}` | Unix timestamp dell'ultima verifica di restore |
| `backuppo_last_verify_success{job="..."}` | `1` se l'ultima verifica è riuscita, `0` altrimenti |

Un job mai eseguito, o mai verificato, non emette la relativa metrica
invece di un valore finto a zero: interrogalo con `absent()` in PromQL, per
esempio per allarmare su un job senza alcuna esecuzione registrata:

```promql
absent(backuppo_last_run_timestamp_seconds{job="documenti"})
```

Come il resto dell'API locale, `/metrics` non richiede autenticazione:
vale la stessa superficie di rete descritta in
[sicurezza](../sicurezza/#superficie-di-rete) — loopback per impostazione
predefinita, tunnel SSH per accedervi da remoto.

## Server MCP

`bkpo mcp --config /etc/backuppo/config.yaml` avvia un server MCP (Model
Context Protocol) **in sola lettura** su stdio, per interrogare lo stato
dei backup da un client MCP come Claude Desktop o Claude Code. Nessun tool
avvia, verifica o ripristina un backup: è un vincolo di design, non
un'omissione temporanea.

Configurazione di esempio per Claude Desktop/Code
(`claude_desktop_config.json` o equivalente):

```json
{
  "mcpServers": {
    "backuppo": {
      "command": "/usr/bin/bkpo",
      "args": ["mcp", "--config", "/etc/backuppo/config.yaml"]
    }
  }
}
```

Tool esposti:

| Tool | Parametri | Restituisce |
| --- | --- | --- |
| `list_jobs` | — | Ogni job configurato: schedule, ultimo esito, ultima verifica |
| `failing_jobs` | — | Solo i job la cui ultima esecuzione o verifica non sono riuscite |
| `job_history` | `job` (obbligatorio), `limit` (opzionale, default 20) | Esecuzioni recenti di quel job, più recenti prima |

Come `/metrics`, richiede `observability.history_path` configurato.

