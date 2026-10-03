---
title: Comparison with other tools
description: How Backuppo compares to Iperius, restic, Borg and Kopia.
---

None of these tools is "better" in an absolute sense: they solve different
problems. This page compares design choices, not speed benchmarks.

## Summary table

| | Backuppo | Iperius Backup | restic | BorgBackup | Kopia |
|---|---|---|---|---|---|
| License | Open source (MIT/Apache-2.0) | Proprietary (free + paid tiers) | Open source (BSD-2) | Open source (BSD-3) | Open source (Apache-2.0) |
| Platforms | Linux, macOS, Windows, Docker | Windows only | Linux, macOS, Windows | Linux, macOS (no native Windows) | Linux, macOS, Windows |
| **Automatic restore verification** | **Yes, schedulable per job** (`verify_restore: every/daily/weekly`) | Vendor-claimed, not independently verifiable | No: only `restic check` (repository integrity, not an actual restore) | No: only `borg check`; Borgmatic adds scheduled repository-level verification | No: policies and scheduling, but no automatic trial restore |
| Built-in scheduler | Yes (daemon) | Yes (Windows service) | No: needs cron/systemd | No: needs cron/systemd (Borgmatic provides it) | Yes (Kopia server) |
| Web UI | Yes, embedded in agent and hub | Yes (Windows desktop) | No (native); Backrest is a third-party UI | No (native); Vorta is a third-party desktop client | Yes (KopiaUI) |
| Deduplication / incremental | Yes, via the built-in Restic engine | Partial (differential/incremental for disk images) | Yes, native | Yes, native | Yes, native (content-defined chunking) |
| Database (native dumps) | PostgreSQL, MySQL/MariaDB, SQLite | SQL Server, MySQL, PostgreSQL, Oracle | No (needs external scripts) | No (needs external scripts) | No (needs external scripts) |
| Disk images / VMs | Disk images, libvirt VMs, Proxmox VMs/containers, VMware VMs | Disk images, VMware ESXi, Hyper-V | No | No | No |
| Centralized multi-site | Optional Hub, agent always stand-alone | Centralized dashboard (paid) | No | No | No |
| Notifications | SMTP, Telegram, ntfy, webhook | Email, central dashboard | None native | None native (Borgmatic: email/webhook) | Email, Pushover, webhook (server mode) |

## Honestly inconvenient notes

- **restic, Borg and Kopia** are mature backup engines, used in production
  for years, with communities much larger than Backuppo's. Backuppo
  **uses restic itself** as an optional incremental engine (see
  [security](../sicurezza/)): it doesn't replace it, it adds scheduling, a
  UI, notifications, a multi-site hub and — above all — automatic restore
  verification, which none of the three offer out of the box.
- **Iperius** covers a wider scope on Windows (VMware/Hyper-V, SQL Server,
  Oracle, a central dashboard) but is closed-source, paid beyond a certain
  usage level, and not cross-platform. Backuppo covers Proxmox and VMware
  (while powered off; Iperius can also back up running VMs).
- If your whole stack already runs on cron + restic/Borg/Kopia and it
  works, **there's no obvious reason to migrate**: Backuppo mainly makes
  sense if you want scheduling, a UI, notifications and restore
  verification without assembling them from separate scripts, or if you
  need to watch multiple sites from a single dashboard.

## Sources

Claims about restic, BorgBackup and Kopia (scheduler, UI, verification) are
checkable against their official docs and independent comparisons published
in 2026; claims about Iperius Backup come from the vendor's own site and
third-party review pages (GetApp, Capterra, TrustRadius), not a hands-on
test.
