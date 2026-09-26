---
title: Quickstart
description: Installazione, prima configurazione ed esecuzione del primo backup con Backuppo.
---

## Installazione

Scarica l'archivio adatto al sistema e `SHA256SUMS` dalla release GitHub.
Verifica l'archivio prima di estrarlo, poi copia `bkpo` in una directory
presente nel `PATH`.

Su Linux:

```sh
grep 'backuppo-v0.1.0-x86_64-unknown-linux-musl.tar.gz' SHA256SUMS | sha256sum --check -
tar -xzf backuppo-v0.1.0-x86_64-unknown-linux-musl.tar.gz
cd backuppo-v0.1.0-x86_64-unknown-linux-musl
sudo install -m 0755 bkpo /usr/local/bin/bkpo
bkpo --version
```

Su macOS usa lo stesso procedimento sostituendo `sha256sum` con
`shasum -a 256`. Su Windows confronta l'hash dello ZIP con
`Get-FileHash .\backuppo-v0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256`
prima di estrarlo.

## Prima configurazione

Crea le directory e parti dall'esempio incluso nella release:

```sh
sudo mkdir -p /etc/backuppo /var/lib/backuppo /var/backups/backuppo
sudo cp examples/config.yaml /etc/backuppo/config.yaml
sudo editor /etc/backuppo/config.yaml
```

I segreti si indicano sempre tramite campi `*_env`. Imposta le variabili prima
di avviare Backuppo, quindi valida la configurazione:

```sh
export BACKUP_PASSPHRASE='una-passphrase-lunga-e-unica'
bkpo check --config /etc/backuppo/config.yaml
```

Esegui un backup e la sua verifica:

```sh
bkpo run --config /etc/backuppo/config.yaml --job home-documents
bkpo verify --config /etc/backuppo/config.yaml --job home-documents
bkpo runs --config /etc/backuppo/config.yaml
```

Se hai configurato `api`, apri `http://127.0.0.1:8787` mentre il daemon è in
esecuzione. Puoi anche avviare soltanto l'interfaccia con
`bkpo serve --config /etc/backuppo/config.yaml`.

## Daemon Linux con systemd

```sh
sudo useradd --system --home /var/lib/backuppo --shell /usr/sbin/nologin backuppo
sudo install -m 0644 deploy/systemd/backuppo.service /etc/systemd/system/
sudo install -m 0600 /dev/null /etc/backuppo/backuppo.env
sudo editor /etc/backuppo/backuppo.env
sudo systemctl daemon-reload
sudo systemctl enable --now backuppo
sudo systemctl status backuppo
```

Il file `backuppo.env` usa righe `NOME=valore`. Proteggilo perché contiene i
segreti risolti dai campi `*_env`.

## Servizio Windows

Apri PowerShell come amministratore ed esegui:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\deploy\windows\install-agent.ps1 -BinaryPath .\bkpo.exe -ConfigPath .\config.yaml
Get-Service Backuppo
```

Lo script copia binario e configurazione, registra il servizio nativo e lo
avvia. Le variabili dei segreti devono essere visibili all'account di sistema,
per esempio come variabili di ambiente della macchina. Per rimuovere il
servizio conservando configurazione e storico:

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

Per sorgenti volume Docker monta anche il socket Docker. Chi ha accesso al
socket ha privilegi equivalenti all'amministratore dell'host; abilitalo solo
quando quella sorgente è necessaria.
