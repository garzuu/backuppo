---
title: Operations and notifications
description: Scheduling, history, verification, retention, reports and daily monitoring.
---

Use the Agent dashboard or the CLI for routine checks:

```sh
bkpo runs --config /etc/backuppo/config.yaml --limit 20
bkpo logs --config /etc/backuppo/config.yaml 42
bkpo snapshots --config /etc/backuppo/config.yaml --job documents
```

Set `max_backup_age_hours` so a previously successful but now stale backup is
reported. `verify_restore` controls automatic recovery tests; `bkpo verify`
runs one immediately.

The daemon applies Restic retention after a successful backup. Append-only
repositories deliberately skip destructive maintenance; run `bkpo maintain`
with separate administrative credentials from a protected environment.

SMTP, Telegram, webhook and ntfy notifications can be assigned independently
to success, failure and verification events. Test them before relying on them:

```sh
bkpo notify-test --config /etc/backuppo/config.yaml
```

To query job status from an existing monitoring stack, see
[integrations](../integrazioni/) (Prometheus).

If the Hub is unavailable, events remain in the local SQLite queue and are
retried. Local backups, verification and notifications continue normally.
