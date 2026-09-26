---
title: How it works
description: Backuppo architecture, the backup lifecycle, and the difference between Agent and Hub.
---

Backuppo separates work that must happen close to the data from centralized
monitoring. The **Agent** reads a source, creates the backup, encrypts it and
writes it to a destination. The optional **Hub** only receives heartbeats,
results and statistics.

```text
source → Agent → compression/encryption → destination
             ↘ restore verification
              ↘ metadata → optional Hub
```

## A job lifecycle

1. The scheduler starts the job according to its five-field cron expression.
2. `pre` hooks prepare the application; a failure stops the job.
3. The Agent collects a folder, database dump, volume, command output, disk
   image or virtual machine.
4. `restic` creates an incremental, deduplicated snapshot; `archive` creates a
   standalone archive that can be compressed and encrypted with age.
5. The result is written to the configured destination.
6. When enabled, Backuppo performs a real restore and checks its integrity.
7. History, notifications and status are updated. Hub events are queued
   without making the job depend on Hub availability.

## Stand-alone and managed modes

In **stand-alone** mode backup, verification, restore, local history, UI and
notifications all work without an account or external service. In managed
mode the Agent opens an outbound connection to the Hub. Backup files and
credentials never reach the Hub. Remote commands are opt-in and limited to
`run` or `verify` for jobs already defined on the Agent.

The Agent UI listens on `127.0.0.1:8787` by default. The Hub listens on port
`8080` and must be placed behind an HTTPS reverse proxy in production.

Continue with the [core concepts](../concetti/) or [installation](../installazione/).
