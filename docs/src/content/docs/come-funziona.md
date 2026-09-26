---
title: Come funziona
description: Architettura di Backuppo, ciclo di un backup e differenza tra Agent e Hub.
---

Backuppo separa il lavoro che deve avvenire vicino ai dati dal controllo
centralizzato. L’**Agent** legge la sorgente, crea il backup, lo cifra e lo
scrive nella destinazione. L’**Hub**, se presente, riceve soltanto heartbeat,
esiti e statistiche.

```text
sorgente → Agent → compressione/cifratura → destinazione
              ↘ verifica restore
               ↘ metadati → Hub opzionale
```

## Il ciclo di un job

1. Lo scheduler avvia il job secondo la sua espressione cron.
2. Gli hook `pre` preparano l’applicazione; un errore interrompe il job.
3. La sorgente viene raccolta: cartella, dump database, volume, comando,
   immagine disco o VM.
4. Il motore `restic` crea uno snapshot incrementale e deduplicato; il motore
   `archive` crea un archivio eventualmente compresso e cifrato con age.
5. Il risultato viene scritto nella destinazione configurata.
6. Se previsto, Backuppo esegue una vera prova di ripristino e ne controlla
   integrità e struttura.
7. Vengono aggiornati storico, stato e notifiche; con un Hub configurato viene
   accodato anche l’evento, senza rendere il job dipendente dall’Hub.

## Due modalità, lo stesso Agent

In modalità **stand-alone** tutte le funzioni di backup, verifica, restore,
storico, UI e notifiche locali restano disponibili. Non serve un account né
un servizio esterno.

In modalità **gestita** l’Agent apre una connessione in uscita verso l’Hub.
L’Hub non monta le sorgenti, non conserva archivi e non riceve credenziali. I
comandi remoti sono disabilitati finché non vengono abilitati esplicitamente
sull’Agent e possono soltanto avviare `run` o `verify` per job già definiti.

## Interfacce

L’Agent espone la UI su `127.0.0.1:8787` per impostazione predefinita. L’Hub
espone la propria UI sulla porta `8080` e in produzione va pubblicato dietro
un reverse proxy HTTPS. La UI dell’Agent non va esposta direttamente su una
rete non fidata.

Prosegui con i [concetti fondamentali](../concetti/) oppure installa il
componente adatto dalla [guida di installazione](../installazione/).
