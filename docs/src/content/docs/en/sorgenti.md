---
title: Sources
description: Configure folders, databases, Docker volumes, commands, disks and virtual machines.
---

Each job has one source. The Agent service account must be able to read its
files or execute the required tool.

- **Folder:** a path plus optional exclusion patterns.
- **SQLite:** the database file itself; coordinate writes with the application.
- **PostgreSQL / MySQL:** Backuppo invokes `pg_dump` or `mysqldump`. Set
  `container` to run the matching client through `docker exec`.
- **Docker volume:** requires access to the Docker daemon. Mounting its socket
  grants privileges comparable to host administration.
- **Command:** stores standard output; arguments are not interpreted by a shell.
- **Disk image:** copies a file or block device byte for byte.
- **libvirt VM:** saves domain XML and disks; shut down or quiesce the VM with
  hooks first.
- **Proxmox VM/container:** runs `vzdump` for the given vmid (QEMU or LXC,
  auto-detected); the agent must run on the Proxmox node itself, with
  direct access to `vzdump`. `mode: snapshot` (default) doesn't stop the
  guest, `suspend` briefly pauses it, `stop` powers it off for the backup.

Database passwords use `password_env`. Ensure the environment variable is
visible to the service, not only to your interactive shell. See the
[configuration reference](../configuration/#sources) for all YAML fields.
