---
title: Sorgenti
description: Configurare cartelle, database, volumi Docker, comandi, dischi e VM.
---

Ogni job ha una sola sorgente. Il processo Agent deve poter leggere i file o
eseguire lo strumento richiesto.

## Cartelle e SQLite

Per una cartella indica il percorso e, se necessario, pattern di esclusione.
Per SQLite seleziona direttamente il file del database. Evita di leggere file
in scrittura senza coordinarti con l’applicazione.

```yaml
source:
  type: folder
  path: /srv/documents
  exclude: ['*.tmp', 'cache/']
```

## PostgreSQL e MySQL/MariaDB

Backuppo usa `pg_dump` o `mysqldump`. Il client deve esistere sull’host; se il
database gira in Docker, valorizza `container` per eseguire il comando con
`docker exec` dentro il container e usare una versione compatibile.

La password è indicata tramite `password_env`. Verifica che la variabile sia
visibile al servizio, non soltanto alla tua shell interattiva.

## Volumi Docker

`docker_volume` acquisisce un volume nominato. Richiede accesso al daemon
Docker; montare il socket nel container Agent equivale, dal punto di vista dei
privilegi, ad amministrare l’host.

## Comandi, dischi e VM

- `command` salva lo standard output come file; gli argomenti non passano da
  una shell.
- `disk_image` copia byte per byte un file o un device a blocchi.
- `libvirt_vm` salva XML e dischi elencati da `virsh`; la VM deve essere
  spenta o resa consistente dagli hook `pre` e `post`.
- `proxmox_vm` esegue `vzdump` per il vmid indicato (QEMU o LXC, rilevato
  automaticamente): richiede che l'agent giri sul nodo Proxmox stesso, con
  accesso diretto a `vzdump`. `mode: snapshot` (default) non ferma la VM,
  `suspend` la sospende brevemente, `stop` la spegne per la durata del
  backup.
- `vmware_vm` esporta OVF+VMDK con `govc export.ovf`; la VM deve essere
  spenta (stesso vincolo di `libvirt_vm`). Richiede `govc` installato e
  autenticato tramite le sue variabili d'ambiente (`GOVC_URL`,
  `GOVC_USERNAME`, `GOVC_PASSWORD`), non tramite la config di Backuppo.

Consulta il [riferimento configurazione](../configuration/#sorgenti) per tutti
i campi e gli esempi YAML.
