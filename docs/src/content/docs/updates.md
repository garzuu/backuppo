---
title: Aggiornamenti sicuri
description: Configurare controllo, staging, installazione e rollback delle release firmate.
---

Backuppo non installa aggiornamenti silenziosamente. Verifica il manifest con
Ed25519, controlla SHA-256 e dimensione del binario, quindi lo conserva in
staging finché un amministratore non ne autorizza l'installazione.

```yaml
updates:
  manifest_url: https://github.com/garzuu/backuppo/releases/latest/download/update-stable.json
  public_key: BASE64_ED25519_PUBLIC_KEY
  channel: stable
  download_dir: /var/lib/backuppo/updates
  install_mode: standalone
  auto_download: false
```

La chiave deve essere ottenuta da un canale indipendente dalla release. HTTP è
accettato soltanto per mirror su localhost.

```sh
bkpo update --config /etc/backuppo/config.yaml check
bkpo update --config /etc/backuppo/config.yaml download
bkpo update --config /etc/backuppo/config.yaml status
bkpo update --config /etc/backuppo/config.yaml apply
bkpo update --config /etc/backuppo/config.yaml rollback
```

`standalone` abilita la sostituzione atomica del binario su Unix. Con
`package` o `docker`, Backuppo segnala e scarica la release ma lascia
l'applicazione al package manager o all'orchestratore. Un container non tenta
mai di sostituire se stesso.

Prima di `apply` la Web UI controlla che non vi siano job attivi. Dopo la
sostituzione è necessario riavviare il servizio e verificarne l'health check.
