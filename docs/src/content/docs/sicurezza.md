---
title: Sicurezza
description: Modello di sicurezza, segreti, esposizione di rete e repository immutabili.
---

## Segreti

Non scrivere password e token nel YAML. I campi `*_env` indicano variabili
d’ambiente; proteggi il file environment con permessi minimi. Conserva
passphrase age o Restic anche fuori dall’host: senza di esse il restore non è
possibile.

## Superficie di rete

- L’Agent ascolta su loopback per impostazione predefinita. Usa un tunnel SSH
  per amministrarlo da remoto.
- `allow_remote: true` serve ai container, ma non aggiunge autenticazione:
  pubblica comunque la porta soltanto su loopback o dietro un controllo
  d’accesso.
- L’Hub parla HTTP e va sempre terminato da un reverse proxy HTTPS.
- L’Agent apre la connessione verso l’Hub; non servono porte inbound sul sito.

## Limitazione dei privilegi

Usa credenziali dedicate per ogni destinazione e sito. Verifica la host key
SFTP. Per S3/Restic preferisci credenziali append-only per il daemon e conserva
quelle di manutenzione in un contesto separato. Object Lock protegge soltanto
se bucket, modalità e retention sono configurati dal provider e verificati con
`storage-check`.

L’accesso al socket Docker equivale sostanzialmente a privilegi amministrativi
sull’host. Montalo soltanto quando devi salvare volumi Docker.

## Hub

I token Agent sono separati per sito e revocabili. Gli utenti hanno ruoli
`read_only`, `operator` o `admin`; assegna `operator` solo a chi può avviare
backup e verifiche. Conserva e sottoponi a backup il database dell’Hub, il
segreto JWT e la chiave di firma delle policy.

## Aggiornamenti

Backuppo accetta manifest firmati Ed25519 e verifica hash e dimensione prima
dello staging. Ottieni la chiave pubblica da un canale indipendente dalla
release e leggi la guida agli [aggiornamenti](../updates/).
