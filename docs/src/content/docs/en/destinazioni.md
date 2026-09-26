---
title: Destinations
description: Configure filesystem, SFTP, S3, WebDAV, cloud drives and Restic.
---

| Need | Suggested destination |
| --- | --- |
| Local test or mounted NAS | Filesystem |
| Remote SSH server | SFTP |
| Object storage or MinIO | S3-compatible |
| DAV server or NAS | WebDAV |
| Google Drive, Dropbox, OneDrive | Cloud drive |
| Large datasets, incrementals and deduplication | Restic |

Keep at least one production copy away from the source machine.

Secret fields contain environment variable names. For example,
`access_key_id_env: BACKUP_S3_KEY` tells Backuppo where to read the value; the
key itself belongs in the service environment or a secret manager.

For SFTP, use a dedicated key and pin `host_key_fingerprint`. For S3, `endpoint`
is optional on AWS and generally required for compatible providers. Path style
is the default; enable virtual-host style only when required.

Restic is the wizard's default. For immutable S3 repositories use append-only
backup credentials and separate maintenance credentials, then verify Object
Lock before production:

```sh
bkpo storage-check --config /etc/backuppo/config.yaml --job documents
```

See the [configuration reference](../configuration/#destinations) for full examples.
