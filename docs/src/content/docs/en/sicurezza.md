---
title: Security
description: Secret handling, network exposure, least privilege and immutable repositories.
---

Never place passwords or tokens directly in YAML. `*_env` fields name
environment variables. Protect the service environment and keep age/Restic
recovery secrets off-host as well.

- The Agent listens on loopback by default. Use an SSH tunnel for remote
  administration.
- `allow_remote: true` does not add authentication; expose a container port on
  loopback or behind external access control.
- The Hub speaks HTTP and must be behind an HTTPS reverse proxy.
- Agents connect outbound, so sites require no inbound Hub port.

Use dedicated credentials per destination and site, pin SFTP host keys, and
prefer append-only backup credentials with separate Restic maintenance
credentials. Docker socket access is equivalent to broad host privileges.

Hub Agent tokens are per-site and revocable. Human roles are `read_only`,
`operator` and `admin`; operators can start backups and verification. Back up
the Hub database and preserve its JWT secret and policy signing key.

Updates require Ed25519-signed manifests and verified hashes. Obtain the
public key independently from the release channel.

## Key rotation

`bkpo keys` handles rotation without requiring you to recreate existing
backups:

- **Restic repositories**: `bkpo keys rotate-restic` adds a new password to
  the repository, authenticating with the current one. Restic doesn't
  re-encrypt anything: it wraps the same master key with an additional
  password, so the operation is instant even on large repositories. The old
  password stays valid until you explicitly remove it with
  `bkpo keys remove-restic` (which requires authenticating with a key other
  than the one being removed — Restic's own protection against locking
  yourself out). Recommended order: `rotate-restic` → update
  `password_env` in the config → `bkpo verify` to confirm the new password
  works → `remove-restic` on the old one. `bkpo keys list-restic` lists the
  keys present with their ids.
- **`archive` engine (age encryption)**: `bkpo keys generate-age` generates a
  new X25519 identity and prints it (never written to disk or config). There
  is no Restic-style rotation here: every archive is encrypted with whichever
  key was active at backup time, and changing `encryption.key_env` doesn't
  re-encrypt archives already written. Keep the old identity until you've
  restored (or redone from scratch) every existing backup, or they become
  unrecoverable.

Either way, the key/password remains the only way to decrypt the data:
losing it means losing the backup.
