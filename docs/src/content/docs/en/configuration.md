---
title: Configuration reference
description: Full schema of the Backuppo YAML file — destinations, sources, jobs and notifiers.
---

The configuration is YAML. Names under `destinations`, `notifiers` and
`jobs` are identifiers chosen by the user. `bkpo check --config FILE`
checks required fields, references and schedules before execution.

## Destinations

### Filesystem

```yaml
destinations:
  local:
    type: fs
    root: /var/backups/backuppo
```

### SFTP

```yaml
  remote:
    type: sftp
    host: storage.example.com
    port: 22
    user: backup
    password_env: SFTP_PASSWORD
    key_path: /etc/backuppo/id_ed25519
    key_passphrase_env: SFTP_KEY_PASSPHRASE
    host_key_fingerprint: SHA256:...
    root: /backups
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

You need at least one of `password_env` and `key_path`. In production,
configure the host key fingerprint.

### S3-compatible

```yaml
  object-storage:
    type: s3
    bucket: backups
    region: eu-central-1
    endpoint: https://s3.example.com
    access_key_id_env: S3_ACCESS_KEY_ID
    secret_access_key_env: S3_SECRET_ACCESS_KEY
    root: site-a
    virtual_host_style: false
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

`endpoint` is optional for AWS. Path style is the default and is usually
what MinIO and self-hosted storage require.

### WebDAV

```yaml
  webdav:
    type: webdav
    url: https://dav.example.com/backups
    user: backup
    password_env: WEBDAV_PASSWORD
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

### Google Drive, Dropbox and OneDrive

All three backends use OAuth. The access token is required; for a
long-running daemon also configure a refresh token, client ID and client
secret:

```yaml
  drive:
    type: google_drive            # or dropbox | one_drive
    root: /backuppo
    access_token_env: DRIVE_ACCESS_TOKEN
    refresh_token_env: DRIVE_REFRESH_TOKEN
    client_id: application-client-id
    client_secret_env: DRIVE_CLIENT_SECRET
    retry: { max_times: 5 }
    bandwidth_limit_kib_s: 5120
```

### Incremental with Restic

Restic keeps deduplicated chunks and incremental snapshots. Backuppo releases
ship a verified version next to `bkpo`; `binary` can select an alternative.
`initialize` creates the repository on the first backup.

```yaml
  incremental:
    type: restic
    repository: s3:https://s3.example.com/backups/restic
    password_env: RESTIC_PASSWORD
    initialize: true
    append_only: true
    environment:
      AWS_ACCESS_KEY_ID: S3_BACKUP_ACCESS_KEY
      AWS_SECRET_ACCESS_KEY: S3_BACKUP_SECRET_KEY
    maintenance_environment:
      AWS_ACCESS_KEY_ID: S3_ADMIN_ACCESS_KEY
      AWS_SECRET_ACCESS_KEY: S3_ADMIN_SECRET_KEY
    object_lock:
      bucket: backups
      region: eu-central-1
      endpoint: https://s3.example.com
      access_key_id_env: S3_LOCK_CHECK_ACCESS_KEY
      secret_access_key_env: S3_LOCK_CHECK_SECRET_KEY
      expected_mode: compliance
      minimum_retention_days: 30
```

Map values are environment variable names, never literal secrets. With
`append_only: true`, the agent never runs `forget` or `prune`. Manual
maintenance exclusively uses `maintenance_environment`:

```bash
bkpo maintain --config /etc/backuppo/config.yaml --job home-documents
bkpo storage-check --config /etc/backuppo/config.yaml --job home-documents
```

`storage-check` signs an S3 `GetObjectLockConfiguration` request and fails when
Object Lock, its mode, or its minimum duration do not match the expected policy.

## Sources

```yaml
source:
  type: folder
  path: /srv/data
  exclude: ["*.tmp", "cache/"]
```

PostgreSQL and MySQL/MariaDB use `pg_dump` and `mysqldump` respectively. If
`container` is present, the command runs inside that container; otherwise
the client must be installed on the host.

```yaml
source:
  type: postgres                 # or mysql
  host: localhost
  port: 5432                     # MySQL: 3306
  user: postgres
  password_env: DATABASE_PASSWORD
  database: app
  container: app-postgres        # optional
```

The other sources are:

```yaml
source: { type: sqlite, path: /srv/app.sqlite }
source: { type: docker_volume, volume: app_data }
source:
  type: command
  command: /usr/local/bin/export-data
  args: ["--format", "json"]
  output_filename: export.json
source: { type: disk_image, path: /dev/disk/by-id/example, output_filename: disk.img }
source: { type: libvirt_vm, name: app-vm }
```

`disk_image` copies a file or device byte for byte. `libvirt_vm` saves the
XML and disks listed by `virsh`; the VM must be powered off to avoid
inconsistent images. `pre`/`post` hooks can handle shutting it down and
restarting it.

## Jobs

```yaml
jobs:
  documents:
    source: { type: folder, path: /srv/documents }
    destination: local
    engine: restic                # wizard default; archive remains compatible
    compression: zstd            # zstd | none; default zstd
    encryption:
      type: age                  # age | none
      passphrase_env: BACKUP_PASSPHRASE
    schedule: "0 3 * * *"       # minute hour day month weekday
    verify_restore: daily        # never | every | daily | weekly
    max_backup_age_hours: 26
    retention:
      daily: 7
      weekly: 4
      monthly: 6
    pre: [/usr/local/bin/before-backup]
    post: [/usr/local/bin/after-backup]
    notify:
      on_success: [ops-webhook]
      on_failure: [ops-email, ops-telegram]
      on_verify: [ops-email]
```

With the `archive` engine, `age` accepts exactly one of `passphrase_env` and
`key_env`. `key_env` must contain an `AGE-SECRET-KEY-...` X25519 private
identity; Backuppo derives its public recipient for encryption and uses the
same identity for restore.

With `engine: restic`, `destination` must point to a `type: restic`
destination; compression and encryption are handled by Restic itself.

A failed `pre` hook aborts the job. A failed `post` hook is recorded, but
does not invalidate an archive that has already been uploaded and
verified.

## Notifiers

```yaml
notifiers:
  ops-telegram:
    type: telegram
    token_env: TELEGRAM_BOT_TOKEN
    chat_id: "123456"
  ops-email:
    type: smtp
    host: smtp.example.com
    port: 587
    user: backups@example.com
    password_env: SMTP_PASSWORD
    from: backups@example.com
    to: [ops@example.com]
  ops-webhook:
    type: webhook
    url: https://hooks.example.com/backuppo
```

The webhook sends JSON with `event`, `job`, `text`, `content` and
`message`, plus archive metadata when available. Suitable for Slack and
Discord; **for ntfy use the `ntfy` type** (below): ntfy treats the body of
a `POST /topic` as the message text, not as JSON.

### ntfy (push to your phone)

```yaml
notifiers:
  ops-ntfy:
    type: ntfy
    url: https://ntfy.sh          # or your self-hosted instance
    topic: backuppo-a1b2c3d4e5    # pick a non-guessable name
    token_env: NTFY_TOKEN         # optional: only for protected topics
```

Publishes to `<url>/<topic>` with title, priority and tags: failures get
high priority (4), successes and verifications normal (3), reports low
(2). On a public topic, anyone who knows the name can read and write: use
a long, random name, or a self-hosted instance with access control and
`token_env`. Messages contain the job name and the error, never backup
contents. To receive them, install the ntfy app (or the Backuppo app,
which opens the topic subscription) and subscribe to the same topic.

### Multi-site hub (opt-in)

The `hub` Cargo feature is included in official binaries. A minimal,
strictly stand-alone agent can be built with `--no-default-features`; in
that build the `hub` type does not exist at config-parsing level.

```yaml
notifiers:
  central-office:
    type: hub
    url: https://hub.example.com
    token_env: HUB_TOKEN
    heartbeat_seconds: 60        # default: 60
    queue_path: /var/lib/backuppo/hub-queue.sqlite  # default: ./backuppo-hub-queue.sqlite
    remote_commands: false       # default: false (see below)
    command_poll_seconds: 15     # default: 15, used only with remote_commands
    policy_public_key: BASE64_ED25519_PUBLIC_KEY
    policy_site_id: 1
    policy_state_path: /var/lib/backuppo/policy-state.json
    policy_poll_seconds: 300
```

The agent only sends metadata to the hub (outcome, bytes, checksum,
error), never backup contents or secrets. When configured, the daemon
(`bkpo daemon`) sends a heartbeat to the hub every `heartbeat_seconds` and,
at the same interval, retries delivery of events left in the local queue
(`queue_path`) with exponential backoff (from 30s up to a cap of one
hour). An unreachable hub never fails a job or stops the daemon: the error
is only logged.

**Remote commands (opt-in).** With `remote_commands: true` the daemon asks
the hub every `command_poll_seconds` whether a "run now" or "verify now"
has been requested by an `admin`/`operator` user (from the app or the
hub's API), executes it and reports the outcome. It remains an
outbound-only connection: the hub never opens connections to the agent.
Security: the command can only choose between backup and verification on
a job **already present in this config** (never an arbitrary command),
respects the per-job lock (the request fails if the job is already
running) and is disabled by default.

With `policy_public_key`, the agent only accepts signed policies scoped by
the hub to its site. Older sequences or reused sequences with different
content are rejected. A non-compliant `block` policy pauses scheduler, UI
starts, and remote commands; `audit` only records violations.

## History, status page and reports

```yaml
observability:
  history_path: /var/lib/backuppo/history.sqlite
  status_page: /var/lib/backuppo/status.html

reports:
  - schedule: "0 8 * * 1"
    days: 7
    notifiers: [ops-email]
```

Without `observability` the agent still works, but does not write history
or an HTML page. Reports require history and run from the daemon.

## API and local Web UI

```yaml
api:
  bind: 127.0.0.1:8787
```

The daemon automatically starts the interface on that address.
Alternatively use `bkpo serve --config config.yaml`. The API exposes
`GET /api/v1/status`, `GET /api/v1/runs`, `GET /api/v1/runs/{id}` and
`POST /api/v1/jobs/{name}/run`. To avoid accidental exposure, the bind
address only accepts loopback IPs. Containers may set `allow_remote: true`,
but the port must be published on loopback or protected by external
authentication. The `POST` request also requires the
`X-Backuppo-UI: 1` header, used by the UI to prevent cross-site triggers.

## Logs

Logs go to **stderr** (stdout is reserved for command results) and are
colorless when stderr is not a terminal, so log files and journald stay
readable. Without `RUST_LOG`:

- `bkpo daemon` and `bkpo serve` log at `info` level for Backuppo crates
  and `warn` for dependencies;
- one-shot commands (`run`, `verify`, `check`, ...) only show `warn` and
  errors.

To change the level set `RUST_LOG`, for example `RUST_LOG=debug` or
`RUST_LOG=info,backuppo_engine=debug`. Watch the names: the binary's
target is `bkpo`, not `backuppo`.
