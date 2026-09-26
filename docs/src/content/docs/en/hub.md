---
title: Agent, Hub and connection
description: When to use the Hub, how to register a site, and how Agents remain autonomous.
---

Use the Hub to monitor multiple installations, customers or sites from one
dashboard. A single machine does not need it: select **Stand-alone** on the
Agent's **Hub connection** page.

The Agent sends heartbeats, job names, results, duration, byte counts,
checksums and errors. It never sends files, archives, source passwords or
destination credentials.

## Register a site

1. In the Hub, select or create a **customer** from the list.
2. Create a **site** under it. The Agent token is shown once; store it safely.
3. On the Agent, open **Hub connection**, select managed mode and follow the
   Identity → Control → Confirm wizard.
4. Enter the Hub HTTPS URL and the name of the environment variable holding
   the token. Add the value to the service environment and restart the Agent.
5. Confirm that the site becomes online.

Returning to stand-alone removes the Hub notifier without deleting jobs or
backups.

Remote commands are opt-in and limited to `run` and `verify` on configured
jobs. Signed policies are bound to a site ID; audit mode records violations,
while block mode can suspend new executions until configuration is compliant.

Continue with [deploying the Hub](../hub-deploy/).
