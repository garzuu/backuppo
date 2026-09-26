---
title: Deploying the multi-site hub
description: How to build, configure and put backuppo-hub into production.
---

The hub (`backuppo-hub`) is a binary separate from the agent, meant to run
on a single control machine and receive only metadata from agents (never
backup contents or secrets). It speaks plain HTTP only: **always put it
behind a reverse proxy that terminates TLS**.

## Build

```sh
cargo build --locked --release --package backuppo-hub
```

Or via Docker:

```sh
docker build -f Dockerfile.hub -t backuppo-hub:local .
```

## Configuration

Copy `examples/hub-config.yaml` and adapt it. Main fields:

- `bind`: listen address (default `0.0.0.0:8080`; fine behind a reverse
  proxy, do not expose it directly to the Internet).
- `database_path`: SQLite file with customers, sites, tokens and
  executions.
- `jwt_secret_env`: environment variable holding the secret used to sign
  access JWTs. Generate it with `openssl rand -hex 32` and never put it
  directly in the config file.
- `policy_signing_key_env`: base64 Ed25519 32-byte seed used only to sign
  policies (generate it with `openssl rand -base64 32`).
- `offline_after_minutes`: threshold after which a site without a
  heartbeat is marked offline and a notification is sent.
- `notifiers` / `notify_on_offline` / `notify_on_failure`: same format as
  the agent's notifiers (telegram/smtp/webhook/ntfy), reused for the
  hub's alerts. `notify_on_offline` fires when a site goes offline,
  `notify_on_failure` as soon as an agent reports a failed job or a
  failed restore verification on `/v1/events` (independently of the
  notifiers the agent itself already used locally).

## First administrator user

There is no public registration endpoint. The first user (and any other)
is created from the command line:

```sh
export HUB_ADMIN_PASSWORD='...'
backuppo-hub create-user \
  --config /etc/backuppo-hub/config.yaml \
  --username admin \
  --password-env HUB_ADMIN_PASSWORD \
  --role admin
```

`--role` accepts `admin` (can create customers/sites/tokens and manage
everything), `operator` (read-only + can request "run now"/"verify now"
on agents, see below), or `read_only` (status and execution read-only).

## Starting it

```sh
export HUB_JWT_SECRET='...'
export HUB_POLICY_SIGNING_KEY='...'
backuppo-hub serve --config /etc/backuppo-hub/config.yaml
```

The Web UI is served on `/`, the API on `/v1/...`. Agents authenticate
only on the `/v1/events` and `/v1/heartbeat` endpoints with the token
generated when the site was created (shown only once); human users use
`/v1/auth/login` to obtain a short-lived JWT access token and a rotating
refresh token.

### Main endpoints

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| POST | `/v1/auth/login` | — | credentials → access/refresh token pair |
| POST | `/v1/auth/refresh` | — | refresh token → new pair (rotation) |
| POST | `/v1/events` | agent token | ingest event (success/failure/verification/report) |
| POST | `/v1/heartbeat` | agent token | mark the site online |
| GET/POST | `/v1/customers` | user / admin | list customers / create |
| GET/POST | `/v1/sites` | user / admin | list sites / create (creation returns the agent token) |
| GET | `/v1/sites/{id}/executions` | user | execution history for the site |
| GET | `/v1/sites/{id}/jobs` | user | distinct job names reported by the site |
| GET/POST | `/v1/sites/{id}/commands` | user / admin, operator | last 50 commands for the site / request `{"kind": "run"\|"verify", "job": "..."}` (409 if already running) |
| GET | `/v1/commands/pending` | agent token | the agent picks up its pending commands (each exactly once) |
| POST | `/v1/commands/{id}/result` | agent token | the agent reports the outcome `{"ok": bool, "detail": "..."}` |
| GET/POST | `/v1/sites/{id}/policies` | user / admin | policy history and signed policy publication |
| GET | `/v1/policy/current` | agent token | current policy for the authenticated site |
| GET | `/v1/policy/public-key` | — | Ed25519 public key to configure on agents |
| POST | `/v1/sites/{id}/tokens` | admin | generate a new agent token for the site (also returns `token_id`) |
| DELETE | `/v1/sites/{id}/tokens/{token_id}` | admin | revoke an agent token |

## Remote commands ("run now", "verify now")

The hub never connects to agents: a request stays queued on the hub until
the agent, which sits behind NAT/firewall, picks it up with its own
outbound poll (`GET /v1/commands/pending`, every
`command_poll_seconds`). It then executes the action and reports the
outcome. States: `pending` → `delivered` → `done` | `failed`, or
`expired`.

- **Opt-in on the agent:** without `remote_commands: true` in the `hub`
  notifier, the agent never picks anything up, and the request expires.
- **Expiry:** a request not picked up within 10 minutes becomes `expired`
  and is never delivered: a backup does not start hours later just
  because someone requested it while the agent was offline.
- **Known actions only:** `run` (backup + retention) and `verify`
  (restore verification), only for jobs already present in the agent's
  config. An unknown or already-running job fails with a clear message.
- **No double starts:** the hub rejects (409) a request identical to one
  already in progress.
- **Permissions:** only `admin` and `operator`; `read_only` gets 403.

## systemd (bare metal)

```sh
sudo useradd --system --home /var/lib/backuppo-hub --shell /usr/sbin/nologin backuppo-hub
sudo install -d -o backuppo-hub -g backuppo-hub /var/lib/backuppo-hub /etc/backuppo-hub
sudo install -m 0644 deploy/systemd/backuppo-hub.service /etc/systemd/system/
sudo install -m 0600 -o backuppo-hub -g backuppo-hub deploy/systemd/backuppo-hub.env.example /etc/backuppo-hub/backuppo-hub.env
# edit /etc/backuppo-hub/backuppo-hub.env and /etc/backuppo-hub/config.yaml
sudo systemctl daemon-reload
sudo systemctl enable --now backuppo-hub
```

## Docker

```sh
docker run -d --name backuppo-hub \
  -v /srv/backuppo-hub/config.yaml:/etc/backuppo-hub/config.yaml:ro \
  -v backuppo-hub-data:/var/lib/backuppo-hub \
  -e HUB_JWT_SECRET='...' \
  -e HUB_POLICY_SIGNING_KEY='...' \
  -p 127.0.0.1:8080:8080 \
  backuppo-hub:local
```

Publish the port only on loopback (as above) and let the reverse proxy
expose it to the Internet.

## Reverse proxy with TLS

### Caddy

```
hub.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Caddy obtains and renews the certificate automatically.

### nginx

```
server {
    listen 443 ssl;
    server_name hub.example.com;

    ssl_certificate     /etc/letsencrypt/live/hub.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/hub.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Backing up the hub itself

`database_path` contains customers, sites, token hashes and the execution
history reported by agents (never backup contents). Include this file in
one of the agent's own jobs, or at least in a separate periodic backup:
if it is lost, only history and records are lost, never the actual
backups (which stay wherever the agents put them).

## Security

- A compromised agent token only allows sending fake events/heartbeats
  for that site, never reading other sites' data or accessing the Web
  UI: it should still be revoked (`DELETE
  /v1/sites/{id}/tokens/{token_id}`) and regenerated if suspected.
- Access JWTs expire in a few minutes (`access_token_minutes`); refresh
  tokens rotate on every use (`POST /v1/auth/refresh` invalidates the
  consumed one and returns a new one).
- A `read_only` role cannot create customers, sites or tokens: only view
  status and executions. An `operator` can additionally trigger backups
  and verifications on agents that enabled remote commands, but cannot
  manage customers, sites or tokens.
- Whoever can create commands can make agents with `remote_commands:
  true` run backups and verifications (not arbitrary commands): assign
  the `operator` role with the same caution as machine access.
