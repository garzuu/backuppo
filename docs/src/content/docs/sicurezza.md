---
title: Sicurezza
description: Modello di sicurezza, segreti, esposizione di rete e repository immutabili.
---

## Segreti

Non scrivere password e token nel YAML. I campi `*_env` indicano variabili
d’ambiente; proteggi il file environment con permessi minimi. Conserva
passphrase age o Restic anche fuori dall’host: senza di esse il restore non è
possibile.

## Rotazione delle chiavi

`bkpo keys` gestisce la rotazione senza richiedere di ricreare i backup
esistenti:

- **Repository Restic**: `bkpo keys rotate-restic` aggiunge una nuova
  password al repository autenticando con quella attuale. Restic non ricifra
  nulla: avvolge la stessa master key con una password aggiuntiva, quindi
  l’operazione è istantanea anche su repository grandi. La vecchia password
  resta valida finché non la rimuovi esplicitamente con
  `bkpo keys remove-restic` (che richiede di autenticarsi con una chiave
  diversa da quella da rimuovere — è una protezione di Restic stesso contro
  l’auto-esclusione). Ordine consigliato: `rotate-restic` → aggiorna
  `password_env` nella config → `bkpo verify` per confermare che la nuova
  password funzioni → `remove-restic` sulla vecchia. `bkpo keys list-restic`
  mostra le chiavi presenti con i rispettivi id.
- **Engine `archive` (cifratura age)**: `bkpo keys generate-age` genera una
  nuova identità X25519 e la stampa (mai scritta su disco o in config). Qui
  non c’è equivalente della rotazione Restic: ogni archivio è cifrato con la
  chiave attiva al momento del backup, e cambiare `encryption.key_env` non
  ricifra gli archivi già scritti. Conserva la vecchia identità finché non
  hai ripristinato (o rifatto da zero) tutti i backup esistenti, altrimenti
  diventano irrecuperabili.

In entrambi i casi la chiave/password rimane l’unico modo per decifrare i
dati: perderla equivale a perdere il backup.

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
