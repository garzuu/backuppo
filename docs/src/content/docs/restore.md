---
title: Restore manuale
description: Come decifrare, decomprimere e ripristinare un backup senza usare bkpo.
---

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
- **PostgreSQL:** il file è `dump.pgcustom`. Ripristinalo in un database vuoto:
  `pg_restore --exit-on-error --no-owner --dbname app dump.pgcustom`.
- **MySQL/MariaDB:** il file è `dump.sql`. Ripristinalo con
  `mysql --user root --password app < dump.sql`.
- **Comando custom:** usa il file indicato da `output_filename` con lo strumento
  che ha prodotto l'export.
- **Immagine disco:** verifica il device di destinazione e scrivi l'immagine
  solo a sistema smontato, per esempio con `dd if=disk.img of=/dev/DEST bs=4M`.
- **VM libvirt:** ripristina i file sotto `disks/`, correggi i percorsi in
  `domain.xml` se necessario e importa con `virsh define domain.xml`.

Esegui sempre il restore prima su un ambiente isolato. Backuppo usa container
temporanei proprio per la verifica automatica dei dump PostgreSQL e MySQL.
