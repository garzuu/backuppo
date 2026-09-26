---
title: Installazione Agent e Hub
description: Installare separatamente Backuppo Agent o Backuppo Hub su Linux, macOS, Windows e Docker.
---

Backuppo viene distribuito in due pacchetti distinti:

- **Backuppo Agent** esegue backup, verifiche e restore sul sistema protetto. Può lavorare stand-alone o collegarsi a un Hub.
- **Backuppo Hub** gestisce utenti, clienti, siti, policy e stato degli agent. Non legge né trasferisce i file sottoposti a backup.

Scarica sempre l'archivio che contiene `agent` oppure `hub` nel nome e verifica
il checksum presente in `SHA256SUMS`.

## Linux

### Agent

```sh
tar -xzf backuppo-agent-vVERSION-x86_64-unknown-linux-musl.tar.gz
cd backuppo-agent-vVERSION-x86_64-unknown-linux-musl
sudo ./install.sh
```

L'installer crea l'utente `backuppo`, installa il servizio systemd e conserva
la configurazione in `/etc/backuppo/config.yaml`. La UI locale risponde su
`http://127.0.0.1:8787`.

### Hub

```sh
tar -xzf backuppo-hub-vVERSION-x86_64-unknown-linux-musl.tar.gz
cd backuppo-hub-vVERSION-x86_64-unknown-linux-musl
sudo ./install.sh
```

Le chiavi JWT e policy vengono generate automaticamente e salvate con
permessi limitati. Prima di esporre la porta 8080 configura un reverse proxy
HTTPS. Crea quindi il primo amministratore:

```sh
export HUB_ADMIN_PASSWORD='una-password-lunga'
sudo -u backuppo-hub --preserve-env=HUB_ADMIN_PASSWORD \
  backuppo-hub create-user --config /etc/backuppo-hub/config.yaml \
  --username admin --password-env HUB_ADMIN_PASSWORD --role admin
```

## macOS

Gli archivi Intel e Apple Silicon contengono LaunchDaemon distinti.

```sh
# Agent
sudo ./install.sh
# UI: http://127.0.0.1:8787
```

```sh
# Hub, dall'archivio backuppo-hub
sudo ./install.sh
# UI: http://127.0.0.1:8080
```

I dati dell'Agent sono in `/Library/Application Support/Backuppo`; quelli
dell'Hub in `/Library/Application Support/Backuppo Hub`. Le chiavi generate
dall'installer Hub vengono mantenute durante gli aggiornamenti.

## Windows

Apri PowerShell come amministratore nella directory estratta:

```powershell
# Archivio Agent
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

```powershell
# Archivio Hub
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

Vengono creati rispettivamente i servizi `Backuppo` e `BackuppoHub`. Config e
database rimangono sotto `%ProgramData%` e non vengono cancellati usando
`install.ps1 -Uninstall`.

## Docker

Usa file Compose separati, così un host Agent non contiene componenti Hub e
viceversa.

### Agent

```sh
cp deploy/docker/agent.env.example .env
docker compose --env-file .env -f deploy/docker/docker-compose.agent.yml up -d
```

L'origine da proteggere è configurata con `BACKUPPO_SOURCE`. La UI viene
pubblicata soltanto su `127.0.0.1:8787`.

### Hub

```sh
cat > .env <<EOF
BACKUPPO_VERSION=latest
HUB_JWT_SECRET=$(openssl rand -hex 32)
HUB_POLICY_SIGNING_KEY=$(openssl rand -base64 32)
EOF
docker compose --env-file .env -f deploy/docker/docker-compose.hub.yml up -d
```

Usa valori distinti e persistenti per `HUB_JWT_SECRET` e
`HUB_POLICY_SIGNING_KEY`. Per creare il primo amministratore:

```sh
docker compose --env-file .env -f deploy/docker/docker-compose.hub.yml run --rm \
  -e HUB_ADMIN_PASSWORD='una-password-lunga' hub create-user \
  --config /etc/backuppo-hub/config.yaml --username admin \
  --password-env HUB_ADMIN_PASSWORD --role admin
```

## Aggiornamenti

Agent e Hub hanno artefatti e immagini separati. Non sostituire mai `bkpo`
con `backuppo-hub` o viceversa. Configurazioni, database e volumi persistenti
sono conservati dagli installer durante gli aggiornamenti.
