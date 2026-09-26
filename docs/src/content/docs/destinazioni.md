---
title: Destinazioni
description: Scegliere e configurare filesystem, SFTP, S3, WebDAV, cloud drive e Restic.
---

## Quale scegliere

| Esigenza | Scelta consigliata |
| --- | --- |
| Prova locale o NAS già montato | Filesystem |
| Server remoto con SSH | SFTP |
| Object storage o MinIO | S3 compatibile |
| Server DAV/NAS | WebDAV |
| Google Drive, Dropbox, OneDrive | Cloud drive |
| Dataset grandi, incrementali e deduplica | Restic |

Per produzione conserva almeno una copia fuori dalla macchina sorgente. Una
cartella locale sullo stesso disco non protegge dal guasto del disco.

## Credenziali

I campi come `password_env`, `access_key_id_env` e `secret_access_key_env`
contengono nomi di variabili. Ad esempio:

```yaml
destinations:
  archivio-s3:
    type: s3
    bucket: backup-azienda
    region: eu-central-1
    access_key_id_env: BACKUP_S3_KEY
    secret_access_key_env: BACKUP_S3_SECRET
    root: sede-roma
```

Le variabili reali vanno nel secret manager, nel file environment protetto del
servizio o nei secret del container.

## SFTP e S3

Con SFTP preferisci una chiave dedicata e imposta
`host_key_fingerprint`; senza fingerprint il server viene accettato con un
avviso. Per S3, `endpoint` è opzionale su AWS e obbligatorio per molti
provider compatibili. Il path style è il default; usa `virtual_host_style`
solo quando richiesto dal provider.

## Restic e immutabilità

Restic è il motore predefinito del wizard. Per repository S3 protetti configura
`append_only: true`, credenziali limitate per i backup e credenziali separate
in `maintenance_environment`. Se usi Object Lock, verificane la policy prima
della messa in produzione:

```sh
bkpo storage-check --config /etc/backuppo/config.yaml --job documenti
```

La configurazione completa è nel [riferimento](../configuration/#destinazioni).
