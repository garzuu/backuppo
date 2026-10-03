---
title: Restore manuale
description: Come decifrare, decomprimere e ripristinare un backup senza usare bkpo.
---

Il percorso consigliato usa le primitive sicure integrate:

```sh
bkpo snapshots --config config.yaml --job documents
bkpo browse --config config.yaml --job documents --snapshot latest
bkpo restore --config config.yaml --job documents --snapshot latest \
  --target /srv/restore-test
```

La destinazione deve essere nuova o vuota; `--overwrite` richiede
un'autorizzazione esplicita. `--dry-run` calcola file e byte senza scrivere, e
`--include percorso` può essere ripetuto per un restore selettivo.

## Restore diretto di un database

Per i job `postgres` e `mysql`, `bkpo restore-db` applica il dump
direttamente a un database di destinazione invece di limitarsi a estrarlo in
una directory. Funziona sia con l'engine `archive` che con `restic`.

```sh
# dentro un container già in esecuzione
bkpo restore-db --config config.yaml --job app-postgres \
  --container app-postgres-nuovo \
  --user app --password-env APP_DB_PASSWORD --database app \
  --yes

# su un server raggiungibile direttamente dall'agent
bkpo restore-db --config config.yaml --job app-postgres \
  --host db.nuovo.example.com --port 5432 \
  --user app --password-env APP_DB_PASSWORD --database app \
  --yes
```

`--yes` è obbligatorio: l'operazione sovrascrive gli oggetti presenti nel
dump nel database di destinazione (per Postgres con `pg_restore
--clean --if-exists`; i dump MySQL includono già `DROP TABLE IF EXISTS` per
default). Oggetti del database di destinazione **non presenti nel dump**
restano intatti: non è un `DROP DATABASE`. `--snapshot` (default `latest`)
sceglie quale backup applicare, come per `bkpo restore`. Per SQLite non
serve un comando dedicato: il file estratto da `bkpo restore` è già il
database.

La procedura seguente resta il percorso di emergenza senza il binario.

Conserva una copia della configurazione e delle variabili dei segreti separata
dai backup. Senza la passphrase `age`, un archivio cifrato non è recuperabile.

## 1. Individuare e scaricare l'archivio

Gli archivi hanno un nome simile a `documents-1760000000.tar.zst.age`. Scarica
il file dalla destination con lo strumento del provider (`scp`, client S3 o
WebDAV). `bkpo verify --config config.yaml --job documents` può verificare
direttamente l'ultimo archivio remoto senza conservarne l'estrazione.

Per un job `engine: restic`, il ripristino manuale equivalente è:

```sh
export RESTIC_REPOSITORY='...'
export RESTIC_PASSWORD='...'
restic snapshots --tag backuppo-documents
restic restore latest --tag backuppo-documents --target restore
```

## 2. Decifrare, decomprimere ed estrarre

Per un archivio cifrato e compresso servono `age`, `zstd` e `tar`:

```sh
mkdir restore
age --decrypt backup.tar.zst.age | zstd --decompress | tar -x -C restore
```

`age` chiede la passphrase in modo interattivo. Per un archivio non cifrato
ometti `age`; per uno non compresso ometti `zstd`:

```sh
tar -xf backup.tar -C restore
zstd --decompress --stdout backup.tar.zst | tar -x -C restore
```

Il file `.backuppo-manifest.json` contiene dimensione e SHA-256 di ciascun file
originale. Può essere conservato come prova di integrità oppure rimosso dopo i
controlli.

## 3. Ripristinare la sorgente

- **Cartella o volume Docker:** copia i file estratti nella destinazione solo
  dopo aver fermato l'applicazione che li usa.
- **SQLite:** il file è `dump.sqlite`. Verificalo con
  `sqlite3 dump.sqlite 'PRAGMA integrity_check;'`, poi sostituisci il database.
- **PostgreSQL:** il file è `dump.pgcustom`. Con il binario disponibile usa
  `bkpo restore-db` (sopra); a mano:
  `pg_restore --exit-on-error --no-owner --clean --if-exists --dbname app dump.pgcustom`.
- **MySQL/MariaDB:** il file è `dump.sql`. Con il binario disponibile usa
  `bkpo restore-db` (sopra); a mano:
  `mysql --user root --password app < dump.sql`.
- **Comando custom:** usa il file indicato da `output_filename` con lo strumento
  che ha prodotto l'export.
- **Immagine disco:** verifica il device di destinazione e scrivi l'immagine
  solo a sistema smontato, per esempio con `dd if=disk.img of=/dev/DEST bs=4M`.
- **VM libvirt:** ripristina i file sotto `disks/`, correggi i percorsi in
  `domain.xml` se necessario e importa con `virsh define domain.xml`.
- **VM/container Proxmox:** il file estratto è l'archivio prodotto da
  `vzdump` (`vzdump-qemu-*.vma.zst` o `vzdump-lxc-*.tar.zst`). Copialo sul
  nodo Proxmox di destinazione e ripristinalo con `qmrestore <file> <nuovo-vmid>`
  (QEMU) o `pct restore <nuovo-vmid> <file>` (LXC).

Esegui sempre il restore prima su un ambiente isolato. Backuppo usa container
temporanei proprio per la verifica automatica dei dump PostgreSQL e MySQL.
