---
title: CLI commands
description: Quick reference for Agent backup, verification, restore and management commands.
---

All Agent commands take `--config FILE`.

| Command | Purpose |
| --- | --- |
| `bkpo check` | Validate YAML, references and schedules |
| `bkpo run --job NAME` | Run backup and retention |
| `bkpo verify --job NAME` | Restore-test the latest backup |
| `bkpo snapshots --job NAME` | List snapshots |
| `bkpo browse --job NAME --snapshot ID` | Browse snapshot contents |
| `bkpo restore --job NAME --snapshot ID --target DIR` | Restore to a directory |
| `bkpo storage-check --job NAME` | Verify S3 Object Lock policy |
| `bkpo maintain --job NAME` | Run privileged Restic retention/prune |
| `bkpo notify-test` | Test one or all notifiers |
| `bkpo runs` / `bkpo logs ID` | Read local history |
| `bkpo daemon` | Start scheduler and configured API/UI |
| `bkpo serve` | Start API/UI only |
| `bkpo update …` | Check, download, apply or roll back an update |

Restore refuses a non-empty target. Use `--dry-run`, repeatable `--include`,
and explicit `--overwrite` when appropriate. Logs are written to stderr; set
`RUST_LOG=info,backuppo_engine=debug` for temporary detail.
