---
title: Wizard and multiple jobs
description: Configure one or more jobs from the Agent's local interface.
---

Open `http://127.0.0.1:8787`. For a remote server, create a tunnel first:

```sh
ssh -L 8787:127.0.0.1:8787 user@server
```

Open **Configuration**. The sidebar lists every job on this installation;
**New** adds a job without replacing existing ones.

## The five steps

1. **Source** — choose what to protect and where it is.
2. **Destination** — select an existing destination or create one.
3. **Schedule** — choose a preset or five-field cron expression and retention.
4. **Protection** — set encryption, compression, restore verification,
   maximum backup age, and notifiers (Telegram, email, webhook, ntfy): create,
   edit and delete them right here, then pick which ones fire on
   `on_success`/`on_failure`/`on_verify`. The `hub` notifier stays managed
   from the **Hub connection** page.
5. **Review** — check the job before server-side validation.

Destinations can be shared. Before changing one, the wizard identifies every
affected job. Use **Duplicate** to create a similar job quickly.

**Save draft** writes the file without changing the running configuration.
**Save and apply** validates it, creates `config.yaml.bak`, replaces the file
atomically and reloads the scheduler. Running jobs are not interrupted. A
change to `api.bind` requires a service restart.

Fields ending in `*_env` contain environment variable names, never secret
values. The wizard does not read or store passwords or tokens.
