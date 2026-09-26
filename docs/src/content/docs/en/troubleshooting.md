---
title: Troubleshooting
description: Diagnose configuration, service, backup, restore and Hub connection problems.
---

| Symptom | Check |
| --- | --- |
| Service does not start | Run `bkpo check --config …`, then inspect journald or Event Viewer |
| Missing environment variable | Services do not inherit your shell; configure their environment or container secrets |
| UI does not open | Check `api.bind`, daemon status and use an SSH tunnel remotely |
| Source permission denied | The service account needs access to the path and every parent directory |
| SFTP fails | Host, port, firewall, credentials and pinned fingerprint |
| S3 signature error | Endpoint, region, addressing style and system clock |
| Database dump fails | Availability/version of `pg_dump` or `mysqldump`; use `container` where useful |
| Restore target rejected | It must be new or empty unless `--overwrite` is explicit |
| Retention is skipped | Expected for append-only repositories; run protected `bkpo maintain` |

If a site stays offline, verify the Agent's HTTPS Hub URL, token environment
variable and clock, then inspect reverse-proxy and Hub logs. Hub downtime must
not fail a backup; events remain in `queue_path`.

Temporarily set `RUST_LOG=debug`, reproduce the problem, and return to the
normal level after collecting the logs.
