---
title: Operatività e notifiche
description: Scheduler, storico, verifiche, retention, report e notifiche quotidiane.
---

## Controlli quotidiani

La pagina iniziale dell’Agent mostra job, ultima esecuzione e stato. Da
terminale:

```sh
bkpo runs --config /etc/backuppo/config.yaml --limit 20
bkpo logs --config /etc/backuppo/config.yaml 42
bkpo snapshots --config /etc/backuppo/config.yaml --job documenti
```

Configura `max_backup_age_hours` per segnalare un backup diventato troppo
vecchio anche se l’ultima esecuzione nota era riuscita.

## Verifiche e retention

`verify_restore` stabilisce la frequenza delle prove automatiche. Puoi sempre
forzarne una dalla UI o con `bkpo verify`. Il daemon applica la retention dopo
un backup Restic riuscito. Con repository append-only esegui la manutenzione
da un ambiente separato e più protetto:

```sh
bkpo maintain --config /etc/backuppo/config.yaml --job documenti
```

## Notifiche

Sono disponibili SMTP, Telegram, webhook e ntfy. Associa canali diversi a
`on_success`, `on_failure` e `on_verify`, quindi prova la configurazione:

```sh
bkpo notify-test --config /etc/backuppo/config.yaml
bkpo notify-test --config /etc/backuppo/config.yaml --notifier ops-email
```

I report periodici richiedono `observability.history_path`; aggregano lo
storico degli ultimi giorni e vengono inviati dal daemon secondo il cron
configurato.

## Quando l’Hub non risponde

Gli eventi restano in una coda SQLite locale e vengono ritentati con backoff.
Backup, verifiche e notifiche locali continuano normalmente: l’Hub non è sul
percorso dei dati né sul percorso critico del job.
