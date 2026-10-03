---
title: Integrations
description: Prometheus metrics, Home Assistant and an MCP server to query backup status from external tools.
---

## Prometheus

The Agent exposes `GET /metrics` in Prometheus text format, on the same
port as the Web UI (`api.bind`, default `127.0.0.1:8787`). Requires
`observability.history_path` to be configured, same as the status page:
returns `503` otherwise.

```yaml
# prometheus.yml
scrape_configs:
  - job_name: backuppo
    metrics_path: /metrics
    static_configs:
      - targets: ['127.0.0.1:8787']
```

For each job, if it has at least one run in the history:

| Metric | Description |
| --- | --- |
| `backuppo_last_run_timestamp_seconds{job="..."}` | Unix timestamp of the last run's start |
| `backuppo_last_run_success{job="..."}` | `1` if successful, `0` otherwise |
| `backuppo_last_run_duration_ms{job="..."}` | Duration in milliseconds |
| `backuppo_last_run_bytes{job="..."}` | Bytes transferred |
| `backuppo_last_verify_timestamp_seconds{job="..."}` | Unix timestamp of the last restore verification |
| `backuppo_last_verify_success{job="..."}` | `1` if the last verification succeeded, `0` otherwise |

A job that has never run, or never been verified, omits the corresponding
metric instead of a fake zero value: query it with `absent()` in PromQL,
for example to alert on a job with no recorded run at all:

```promql
absent(backuppo_last_run_timestamp_seconds{job="documents"})
```

Like the rest of the local API, `/metrics` requires no authentication: the
same network exposure described in [security](../sicurezza/) applies —
loopback by default, an SSH tunnel to reach it remotely.

## MCP server

`bkpo mcp --config /etc/backuppo/config.yaml` starts a **read-only** MCP
(Model Context Protocol) server over stdio, to query backup status from an
MCP client such as Claude Desktop or Claude Code. No tool starts, verifies
or restores a backup: that's a design constraint, not a temporary gap.

Sample configuration for Claude Desktop/Code
(`claude_desktop_config.json` or equivalent):

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

Exposed tools:

| Tool | Parameters | Returns |
| --- | --- | --- |
| `list_jobs` | — | Every configured job: schedule, last outcome, last verification |
| `failing_jobs` | — | Only jobs whose last run or verification did not succeed |
| `job_history` | `job` (required), `limit` (optional, default 20) | Recent runs for that job, most recent first |

Like `/metrics`, this requires `observability.history_path` to be
configured.

