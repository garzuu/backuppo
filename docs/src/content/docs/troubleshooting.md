---
title: Risoluzione dei problemi
description: Diagnosi rapida di configurazione, servizi, backup, restore e collegamento Hub.
---

## Configurazione e servizio

| Sintomo | Controllo |
| --- | --- |
| Il servizio non parte | Esegui `bkpo check --config …`, poi consulta `journalctl -u backuppo` o Event Viewer |
| Variabile mancante | Il servizio non eredita la shell: inseriscila nel file environment o nei secret del container |
| La UI non si apre | Controlla `api.bind`, che il daemon sia attivo e usa un tunnel SSH se remoto |
| La modifica non è attiva | Usa **Salva e applica**; un cambio di `api.bind` richiede riavvio |

## Backup e destinazioni

| Sintomo | Controllo |
| --- | --- |
| `permission denied` sulla sorgente | L’utente del servizio deve poter leggere percorso e directory superiori |
| SFTP non raggiungibile | Host, porta, firewall, chiave/password e fingerprint |
| Errore firma S3 | Endpoint, regione, path/virtual-host style e orologio del sistema |
| Dump database fallito | Versione e disponibilità di `pg_dump`/`mysqldump`; usa `container` se opportuno |
| Volume Docker non visibile | Nome volume e accesso dell’Agent al daemon Docker |

## Restore e Restic

| Sintomo | Controllo |
| --- | --- |
| Target rifiutato | La directory deve essere nuova o vuota; usa `--overwrite` consapevolmente |
| Snapshot non trovato | Esegui `bkpo snapshots` sullo stesso job/repository |
| Repository bloccato | Verifica che non vi siano processi Restic attivi prima di intervenire |
| Retention non eseguita | Con `append_only` è intenzionale: usa `bkpo maintain` con credenziali separate |

## Hub

Se il sito resta offline, verifica dall’Agent l’URL HTTPS, la variabile del
token e l’orologio. Controlla poi proxy e log dell’Hub. Un Hub irraggiungibile
non deve far fallire il job: gli eventi restano nella coda indicata da
`queue_path`.

Per raccogliere più dettagli imposta temporaneamente `RUST_LOG=debug`, ripeti
l’operazione e rimuovi il livello dettagliato dopo la diagnosi.
