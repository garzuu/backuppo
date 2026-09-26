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
