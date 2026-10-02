---
title: Threat model
description: What Backuppo protects, from whom, and where protection stops.
---

This page honestly lists what Backuppo protects and what it doesn't. It's
not a formal security audit (none has been commissioned), but how the
maintainer reasons about the design choices.

## Scenario 1 — the source host is compromised

An attacker gets code execution on the host the agent is protecting, with
the same privileges as the agent process.

**What they can do:** read the source files for the next backup, read the
environment variables resolved from `*_env` (destination password, age
passphrase, Restic password), write corrupted or fake backups to the
destination.

**What they can't do, if configured correctly:**
- Delete or alter backups **already written**, if the Restic repository is
  `append_only: true` with write credentials separate from maintenance
  credentials (see [security](../sicurezza/)).
  Maintenance credentials (`maintenance_environment`) are never loaded by
  the daemon: an attacker reading the process environment won't find them.
- Delete backups on an S3 destination with Object Lock in `compliance`
  mode within the retention period, checkable with `bkpo storage-check`:
  not even the bucket's admin credentials allow this until retention
  expires.
- Read already-written, encrypted backups without the encryption key: if
  `encryption.key_env`/`passphrase_env` points to a variable unresolvable
  from the compromised host (e.g. injected only at runtime by a secret
  manager and never persisted), historical data stays protected.

**What you can't avoid:** a host compromised **before** the next backup can
have fake data written or new data exfiltrated. `verify_restore` detects
corruption and structural inconsistencies, not the silent exfiltration of
genuine data nor the insertion of fake-but-structurally-valid data. If you
suspect a compromised host: revoke the destination credentials that agent
used, rotate encryption keys (see
[key rotation](../sicurezza/#key-rotation)), and treat every
backup made after the compromise as untrusted until you verify it by hand.

## Scenario 2 — only the destination/storage is compromised

An attacker gets admin credentials for the S3 bucket, SFTP server or WebDAV
provider, but doesn't touch the agent.

**What they can do:** read the encrypted archives (without the key, not the
content), delete them if there's no Object Lock or if the mode is
`governance` rather than `compliance`.

**What they can't do:** decrypt `age` archives or a Restic repository
without the matching key — that key is never on the destination.

## Scenario 3 — the Hub is compromised

An attacker gets code execution on the Hub host, or direct access to its
database.

**What they can do:** read metadata for every execution (job names,
outcomes, durations, errors — never backup content, which the Hub never
receives), request `run`/`verify` on agents with `remote_commands: true`
enabled (never arbitrary commands), and — the honestly inconvenient part —
**sign malicious policies** if they also get `policy_signing_key_env`: the
signing key lives on the same host as the Hub, not in a separate HSM. A
fully compromised Hub can therefore, in `block` mode, suspend the scheduler
and UI of connected agents (a denial of service), but can't run arbitrary
code or read backup content.

**Practical mitigation:** treat the Hub host with the same care as a host
that handles secrets — limited access, patching, no other service exposed
on the same machine. A compromised agent token alone (without compromising
the Hub) only allows fake events/heartbeats for that one site, never access
to other sites or the Web UI.

## Scenario 4 — network interception

**Protected:** every remote destination (SFTP, S3, WebDAV, cloud drive)
uses TLS via `rustls`; SFTP supports host key verification
(`host_key_fingerprint`). Archives are already encrypted before leaving the
host, so even an attacker intercepting plaintext traffic (TLS compromised
or absent) only gets encrypted data — unless the destination is local `fs`
without `age` encryption enabled, in which case the data was never
encrypted in the first place.

**Not protected:** the Hub speaks plain HTTP and **must** sit behind a
reverse proxy that terminates TLS (see
[deploying the Hub](../hub-deploy/#reverse-proxy-with-tls)); exposing it
without that leaks login credentials and tokens in the clear.

## Scenario 5 — release supply-chain compromise

An attacker gets `UPDATE_SIGNING_KEY_B64` (the CI secret used to sign
`update-stable.json`).

**What they can do:** publish an update manifest that `bkpo update` would
accept as legitimate, if they can also get agents to download it (requires
controlling `manifest_url` or compromising distribution).

**Mitigation:** the public key (`updates.public_key`) must be distributed
**independently** from the release channel (ideally not in the same Git
repository) — see [updates](../updates/). If you suspect the private key
is compromised: revoke the CI secret, generate a new keypair, distribute
the new public key out of band, and disclose the incident. There is
currently no automatic agent-side revocation mechanism.

## Explicitly out of scope

- A host already compromised with root/administrator privileges before
  Backuppo was installed: whoever has that access can already read
  anything the agent process reads.
- Supply-chain attacks against upstream dependencies (Rust crates, Restic,
  age): this project relies on RustSec advisories and those projects'
  verified releases, with no independent auditing of its own.
- Physical access to the machine.
- Side-channel attacks against the cryptographic implementations used
  (`age`, Restic): assumed correct, not independently re-verified by this
  project.

To report an actual vulnerability (not a theoretical scenario), see
[SECURITY.md](https://github.com/garzuu/backuppo/blob/main/SECURITY.md).
