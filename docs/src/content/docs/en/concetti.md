---
title: Concepts
description: The terminology used by Backuppo's interface and documentation.
---

## Agent

The service installed on a machine that can read the protected data. It runs
jobs, restores and verification, and keeps working when the Hub is offline.

## Job

A source, destination, schedule, retention policy and verification/notification
rules. One Agent installation can contain any number of independent jobs.

## Source and destination

A **source** is the data to protect: folder, PostgreSQL, MySQL/MariaDB, SQLite,
Docker volume, command output, disk image or libvirt VM. A **destination** is
where the backup is stored: filesystem, SFTP, S3, WebDAV, cloud drive or a
Restic repository. Several jobs may share one destination.

## Engine

`restic` creates incremental, deduplicated snapshots and is the wizard's
default. `archive` creates standalone files and retains compatibility with
Backuppo's native archive format.

## Restore verification and retention

Restore verification performs an actual recovery test; it is more than an
existence check. Retention defines how many daily, weekly and monthly
snapshots remain. Append-only Restic repositories require separate maintenance
credentials for destructive cleanup.

## Hub, customer and site

The Hub groups Agents. A **customer** is an organizational container; a
**site** is one Agent installation and owns one or more revocable tokens.
