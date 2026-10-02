---
title: Comandi CLI
description: Riferimento rapido dei comandi bkpo per backup, verifica, restore e gestione.
---

Tutti i comandi Agent ricevono il file con `--config FILE`.

| Comando | Funzione |
| --- | --- |
| `bkpo check` | Valida YAML, riferimenti e schedule |
| `bkpo run --job NOME` | Esegue backup e retention |
| `bkpo verify --job NOME` | Verifica l’ultimo backup con un restore |
| `bkpo snapshots --job NOME` | Elenca gli snapshot |
| `bkpo browse --job NOME --snapshot ID` | Elenca il contenuto |
| `bkpo restore --job NOME --snapshot ID --target DIR` | Ripristina in una directory |
| `bkpo storage-check --job NOME` | Verifica la policy S3 Object Lock |
| `bkpo maintain --job NOME` | Esegue retention/prune Restic privilegiata |
| `bkpo keys generate-age` | Genera una nuova identità age X25519 |
| `bkpo keys list-restic --job NOME` | Elenca le chiavi del repository Restic |
| `bkpo keys rotate-restic --job NOME` | Aggiunge una nuova password senza ricifrare |
| `bkpo keys remove-restic --job NOME --key-id ID` | Rimuove una chiave Restic |
| `bkpo notify-test` | Prova uno o tutti i notifier |
| `bkpo runs` / `bkpo logs ID` | Consulta lo storico locale |
| `bkpo daemon` | Avvia scheduler, API e UI configurata |
| `bkpo serve` | Avvia soltanto API e UI |
| `bkpo update …` | Controlla, scarica, applica o annulla un update |

## Restore sicuro

`restore` rifiuta una directory non vuota. Usa `--dry-run` per vedere file e
byte, `--include PERCORSO` (ripetibile) per una selezione e `--overwrite` solo
dopo aver verificato il target.

## Log

I log vanno su stderr. Per aumentare il dettaglio:

```sh
RUST_LOG=info,backuppo_engine=debug bkpo run \
  --config /etc/backuppo/config.yaml --job documenti
```
