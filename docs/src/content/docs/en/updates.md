---
title: Secure updates
description: Configure signed release checks, staging, installation and rollback.
---

Backuppo never installs an update silently. It verifies the manifest with
Ed25519, checks the binary SHA-256 and size, and stages it until an
administrator authorizes installation.

```yaml
updates:
  manifest_url: https://github.com/garzuu/backuppo/releases/latest/download/update-stable.json
  public_key: BASE64_ED25519_PUBLIC_KEY
  channel: stable
  download_dir: /var/lib/backuppo/updates
  install_mode: standalone
  auto_download: false
```

Obtain the public key through a channel independent from the release. Plain
HTTP is accepted only for localhost mirrors.

```sh
bkpo update --config /etc/backuppo/config.yaml check
bkpo update --config /etc/backuppo/config.yaml download
bkpo update --config /etc/backuppo/config.yaml status
bkpo update --config /etc/backuppo/config.yaml apply
bkpo update --config /etc/backuppo/config.yaml rollback
```

`standalone` enables atomic binary replacement on Unix. With `package` or
`docker`, Backuppo reports and stages the release but leaves deployment to the
package manager or orchestrator. A container never replaces itself.
