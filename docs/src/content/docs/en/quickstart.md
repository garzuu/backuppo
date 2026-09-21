---
title: Quickstart
description: Installation, first configuration and running your first backup with Backuppo.
---

## Installation

Download the archive for your platform and `SHA256SUMS` from the GitHub
release. Verify the archive before extracting it, then copy `bkpo` to a
directory on your `PATH`.

On Linux:

```sh
grep 'backuppo-v0.1.0-x86_64-unknown-linux-musl.tar.gz' SHA256SUMS | sha256sum --check -
tar -xzf backuppo-v0.1.0-x86_64-unknown-linux-musl.tar.gz
cd backuppo-v0.1.0-x86_64-unknown-linux-musl
sudo install -m 0755 bkpo /usr/local/bin/bkpo
bkpo --version
```

On macOS use the same procedure, replacing `sha256sum` with
`shasum -a 256`. On Windows compare the ZIP's hash with
`Get-FileHash .\backuppo-v0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256`
before extracting it.

## First configuration

Create the directories and start from the example included in the release:

```sh
sudo mkdir -p /etc/backuppo /var/lib/backuppo /var/backups/backuppo
sudo cp examples/config.yaml /etc/backuppo/config.yaml
sudo editor /etc/backuppo/config.yaml
```

Secrets are always referenced through `*_env` fields. Set the variables
before starting Backuppo, then validate the configuration:

```sh
export BACKUP_PASSPHRASE='a-long-unique-passphrase'
bkpo check --config /etc/backuppo/config.yaml
```

Run a backup and verify it:

```sh
bkpo run --config /etc/backuppo/config.yaml --job home-documents
bkpo verify --config /etc/backuppo/config.yaml --job home-documents
bkpo runs --config /etc/backuppo/config.yaml
```

If you configured `api`, open `http://127.0.0.1:8787` while the daemon is
running. You can also start just the interface with
`bkpo serve --config /etc/backuppo/config.yaml`.

## Linux daemon with systemd

```sh
sudo useradd --system --home /var/lib/backuppo --shell /usr/sbin/nologin backuppo
sudo install -m 0644 deploy/systemd/backuppo.service /etc/systemd/system/
sudo install -m 0600 /dev/null /etc/backuppo/backuppo.env
sudo editor /etc/backuppo/backuppo.env
sudo systemctl daemon-reload
sudo systemctl enable --now backuppo
sudo systemctl status backuppo
```

The `backuppo.env` file uses `NAME=value` lines. Protect it, since it
contains the secrets resolved from `*_env` fields.

## Windows service

Open PowerShell as administrator and run:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\deploy\windows\install-service.ps1 -BinaryPath .\bkpo.exe -ConfigPath .\config.yaml
Get-Service Backuppo
```

The script copies the binary and configuration, registers the native
service and starts it. Secret variables must be visible to the system
account, for example as machine-wide environment variables. To remove the
service while keeping the configuration and history:

```powershell
.\deploy\windows\install-service.ps1 -Uninstall
```

## Container

```sh
docker build -t backuppo .
docker run --rm \
  --env-file /etc/backuppo/backuppo.env \
  -v /etc/backuppo/config.yaml:/etc/backuppo/config.yaml:ro \
  -v /var/lib/backuppo:/var/lib/backuppo \
  -v /srv/data:/data:ro \
  backuppo
```

For Docker volume sources also mount the Docker socket. Whoever has access
to the socket has privileges equivalent to the host administrator; enable
it only when that source is actually needed.
