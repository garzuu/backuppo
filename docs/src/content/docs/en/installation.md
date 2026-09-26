---
title: Agent and Hub installation
description: Install Backuppo Agent or Backuppo Hub separately on Linux, macOS, Windows, and Docker.
---

Backuppo ships as two distinct products: the **Agent** runs backups on the
protected host, while the **Hub** centrally manages sites, users, policies,
and status without reading backup contents.

Release assets are named `backuppo-agent-*` and `backuppo-hub-*`. Verify the
selected archive against `SHA256SUMS` before installing it.

## Linux and macOS

Extract the archive for the product and architecture, then run:

```sh
sudo ./install.sh
```

On Linux this installs a hardened systemd service. On macOS it installs a
LaunchDaemon. The Agent UI listens on `127.0.0.1:8787`; the Hub UI listens on
port 8080 and should be placed behind an HTTPS reverse proxy when exposed.

## Windows

Open an elevated PowerShell in the extracted Agent or Hub archive:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

The installers create separate `Backuppo` and `BackuppoHub` Windows services.

## Docker

Use the independent Compose files:

```sh
docker compose --env-file .env -f deploy/docker/docker-compose.agent.yml up -d
docker compose --env-file .env -f deploy/docker/docker-compose.hub.yml up -d
```

Start from `agent.env.example` or `hub.env.example`. Agent and Hub use
different images, configuration volumes, data volumes, and secrets.
