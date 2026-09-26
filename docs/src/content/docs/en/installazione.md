---
title: Installing Agent and Hub
description: Install Backuppo Agent or Backuppo Hub separately on Linux, macOS, Windows and Docker.
---

Backuppo ships as two separate products:

- **Agent** runs backups, verification and restore. It works stand-alone or
  connects to a Hub.
- **Hub** manages users, customers, sites, policies and Agent status. It never
  reads or transports backup data.

Always download the archive containing `agent` or `hub` in its name and check
it against `SHA256SUMS`.

## Linux and macOS

Extract the platform archive and run its installer:

```sh
# From an Agent archive
sudo ./install.sh

# Or, from a Hub archive
sudo ./install.sh
```

Linux uses systemd; macOS uses separate LaunchDaemons. The Agent UI is on
`http://127.0.0.1:8787`. The Hub UI is on `http://127.0.0.1:8080` until it is
published behind HTTPS.

## Windows

Open PowerShell as administrator in the extracted directory:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

The Agent and Hub archives install distinct `Backuppo` and `BackuppoHub`
services. Configuration and databases remain in `%ProgramData%` on upgrades
and uninstall.

## Docker

Use the separate Compose files:

```sh
docker compose --env-file .env -f deploy/docker/docker-compose.agent.yml up -d
docker compose --env-file .env -f deploy/docker/docker-compose.hub.yml up -d
```

For the Hub, persist distinct `HUB_JWT_SECRET` and
`HUB_POLICY_SIGNING_KEY` values, then create the first administrator as shown
in the [Hub deployment guide](../hub-deploy/).

Agent and Hub artifacts are not interchangeable. Install the Agent on every
machine that can access protected data and only one Hub for centralized
monitoring.
