---
title: Confronto con altri strumenti
description: Come si posiziona Backuppo rispetto a Iperius, restic, Borg e Kopia.
---

Nessuno di questi strumenti è "migliore" in assoluto: risolvono problemi
diversi. Questa pagina confronta le scelte di design, non i benchmark di
velocità.

## Tabella riassuntiva

| | Backuppo | Iperius Backup | restic | BorgBackup | Kopia |
|---|---|---|---|---|---|
| Licenza | Open source (MIT/Apache-2.0) | Proprietario (free + a pagamento) | Open source (BSD-2) | Open source (BSD-3) | Open source (Apache-2.0) |
| Piattaforme | Linux, macOS, Windows, Docker | Solo Windows | Linux, macOS, Windows | Linux, macOS (no Windows nativo) | Linux, macOS, Windows |
| **Verifica restore automatica** | **Sì, pianificabile per job** (`verify_restore: every/daily/weekly`) | Dichiarata dal vendor, non verificabile indipendentemente | No: solo `restic check` (integrità repository, non un restore reale) | No: solo `borg check`; Borgmatic aggiunge verifica pianificata a livello di repository | No: policy e scheduling, ma senza un restore di prova automatico |
| Scheduler incorporato | Sì (daemon) | Sì (servizio Windows) | No: serve cron/systemd | No: serve cron/systemd (Borgmatic lo fornisce) | Sì (Kopia server) |
| Web UI | Sì, incorporata in agent e hub | Sì (desktop Windows) | No (nativa); Backrest è una UI di terze parti | No (nativa); Vorta è un client desktop di terze parti | Sì (KopiaUI) |
| Deduplica / incrementale | Sì, via motore Restic integrato | Parziale (differenziale/incrementale per immagini disco) | Sì, nativa | Sì, nativa | Sì, nativa (content-defined chunking) |
| Database (dump nativi) | PostgreSQL, MySQL/MariaDB, SQLite | SQL Server, MySQL, PostgreSQL, Oracle | No (richiede script esterni) | No (richiede script esterni) | No (richiede script esterni) |
| Immagini disco / VM | Immagini disco, VM libvirt, VM/container Proxmox, VM VMware | Immagini disco, VMware ESXi, Hyper-V | No | No | No |
| Multi-sito centralizzato | Hub opzionale, agent sempre stand-alone | Pannello centralizzato (a pagamento) | No | No | No |
| Notifiche | SMTP, Telegram, ntfy, webhook | Email, pannello centrale | Nessuna nativa | Nessuna nativa (Borgmatic: email/webhook) | Email, Pushover, webhook (in modalità server) |

## Note onestamente scomode

- **restic, Borg e Kopia** sono strumenti di motore di backup maturi,
  largamente usati in produzione da anni, con community molto più grandi di
  Backuppo. Backuppo **usa restic stesso** come motore incrementale
  opzionale (vedi [sicurezza](../sicurezza/)): non lo sostituisce, aggiunge
  scheduling, UI, notifiche, hub multi-sito e — soprattutto — la verifica
  automatica del restore che nessuno dei tre offre out-of-the-box.
- **Iperius** copre uno scope più ampio su Windows (VMware/Hyper-V, SQL
  Server, Oracle, pannello centralizzato) ma è closed-source, a pagamento
  oltre un certo uso, e non multipiattaforma. Backuppo copre Proxmox e
  VMware (a VM spenta; Iperius può anche su VM accese).
- Se il tuo intero stack è già su cron + restic/Borg/Kopia e funziona,
  **non c'è un motivo ovvio per migrare**: Backuppo ha senso soprattutto se
  vuoi scheduler, UI, notifiche e verifica restore senza assemblarli da
  script separati, o se devi sorvegliare più siti da un unico pannello.

## Fonti

Le affermazioni su restic, BorgBackup e Kopia (scheduler, UI, verifica)
sono verificabili nella rispettiva documentazione ufficiale e in confronti
indipendenti pubblicati nel 2026; quelle su Iperius Backup sono tratte dal
sito del vendor e da pagine di recensioni di terze parti (GetApp, Capterra,
TrustRadius), non da un test diretto.
