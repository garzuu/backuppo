---
title: Manual restore
description: How to decrypt, decompress and restore a backup without using bkpo.
---

The recommended path uses Backuppo's safe recovery primitives:

```sh
bkpo snapshots --config config.yaml --job documents
bkpo browse --config config.yaml --job documents --snapshot latest
bkpo restore --config config.yaml --job documents --snapshot latest \
  --target /srv/restore-test
```

The target must be new or empty; `--overwrite` requires explicit approval.
`--dry-run` counts files and bytes without writing, and `--include path` can be
repeated for selective recovery.

## Restoring a database directly

For `postgres` and `mysql` jobs, `bkpo restore-db` applies the dump
directly to a destination database instead of just extracting it to a
directory. Works with both the `archive` and `restic` engines.

```sh
# into an already-running container
bkpo restore-db --config config.yaml --job app-postgres \
  --container app-postgres-new \
  --user app --password-env APP_DB_PASSWORD --database app \
  --yes

# into a server the agent can reach directly
bkpo restore-db --config config.yaml --job app-postgres \
  --host db.new.example.com --port 5432 \
  --user app --password-env APP_DB_PASSWORD --database app \
  --yes
```

`--yes` is required: the operation overwrites objects present in the dump
inside the destination database (for Postgres via `pg_restore
--clean --if-exists`; MySQL dumps already include `DROP TABLE IF EXISTS`
by default). Objects in the destination database **not present in the
dump** are left untouched: this is not a `DROP DATABASE`. `--snapshot`
(default `latest`) picks which backup to apply, same as `bkpo restore`.
SQLite needs no dedicated command: the file extracted by `bkpo restore` is
already the database.

The following procedure remains the emergency path without the binary.

Keep a copy of the configuration and secret variables separate from the
backups. Without the `age` passphrase, an encrypted archive cannot be
recovered.

## 1. Find and download the archive

Archives have a name like `documents-1760000000.tar.zst.age`. Download the
file from the destination using the provider's tool (`scp`, an S3 or
WebDAV client). `bkpo verify --config config.yaml --job documents` can
verify the latest remote archive directly, without keeping the
extraction.

For an `engine: restic` job, the equivalent manual restore is:

```sh
export RESTIC_REPOSITORY='...'
export RESTIC_PASSWORD='...'
restic snapshots --tag backuppo-documents
restic restore latest --tag backuppo-documents --target restore
```

## 2. Decrypt, decompress and extract

For an encrypted, compressed archive you need `age`, `zstd` and `tar`:

```sh
mkdir restore
age --decrypt backup.tar.zst.age | zstd --decompress | tar -x -C restore
```

`age` prompts for the passphrase interactively. For an unencrypted archive
skip `age`; for an uncompressed one skip `zstd`:

```sh
tar -xf backup.tar -C restore
zstd --decompress --stdout backup.tar.zst | tar -x -C restore
```

The `.backuppo-manifest.json` file contains the size and SHA-256 of each
original file. It can be kept as proof of integrity or removed after the
checks.

## 3. Restore the source

- **Folder or Docker volume:** copy the extracted files to the
  destination only after stopping the application that uses them.
- **SQLite:** the file is `dump.sqlite`. Verify it with
  `sqlite3 dump.sqlite 'PRAGMA integrity_check;'`, then replace the
  database.
- **PostgreSQL:** the file is `dump.pgcustom`. With the binary available
  use `bkpo restore-db` (above); by hand:
  `pg_restore --exit-on-error --no-owner --clean --if-exists --dbname app dump.pgcustom`.
- **MySQL/MariaDB:** the file is `dump.sql`. With the binary available use
  `bkpo restore-db` (above); by hand: `mysql --user root --password app < dump.sql`.
- **Custom command:** use the file named by `output_filename` with the
  tool that produced the export.
- **Disk image:** check the destination device and write the image only
  while unmounted, for example with `dd if=disk.img of=/dev/DEST bs=4M`.
- **libvirt VM:** restore the files under `disks/`, fix the paths in
  `domain.xml` if needed, and import with `virsh define domain.xml`.

Always try the restore on an isolated environment first. Backuppo uses
disposable containers precisely to automatically verify PostgreSQL and
MySQL dumps.
