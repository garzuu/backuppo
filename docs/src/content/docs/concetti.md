---
title: Concetti
description: Le parole usate nell’interfaccia e nella documentazione di Backuppo.
---

## Agent

Il servizio installato sulla macchina che può leggere i dati. Esegue job,
restore e verifiche. È autonomo e continua a lavorare se l’Hub non è
raggiungibile.

## Job

L’unità di configurazione: una sorgente, una destinazione, una pianificazione,
una politica di conservazione e le regole di verifica/notifica. Una singola
installazione può contenere più job indipendenti.

## Sorgente

Ciò che viene protetto: cartella, PostgreSQL, MySQL/MariaDB, SQLite, volume
Docker, output di un comando, immagine disco o VM libvirt.

## Destinazione

Dove viene conservato il backup. Può essere una cartella locale o montata,
SFTP, S3 compatibile, WebDAV, un cloud drive o un repository Restic. Una
destinazione può essere condivisa da più job.

## Motore

`restic` produce snapshot incrementali e deduplicati ed è la scelta
predefinita del wizard. `archive` produce file autonomi e mantiene la
compatibilità con il formato nativo Backuppo.

## Verifica restore

Una prova automatica di ripristino. `every`, `daily`, `weekly` e `never`
stabiliscono quando eseguirla. È distinta dal semplice controllo che il file
di backup esista.

## Retention

Quanti snapshot giornalieri, settimanali e mensili conservare. Nei repository
Restic `append_only` l’Agent non può eliminare dati: la manutenzione usa
credenziali amministrative separate.

## Hub, cliente e sito

L’Hub raggruppa gli Agent. Un **cliente** è il contenitore organizzativo; un
**sito** rappresenta una specifica installazione Agent e possiede uno o più
token revocabili. L’identificativo viene scelto dalla UI, non va inventato o
copiato manualmente dall’utente.
