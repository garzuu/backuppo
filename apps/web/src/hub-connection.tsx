import React, { useEffect, useMemo, useState } from "react";
import yaml from "js-yaml";

type ObjectMap = Record<string, unknown>;
type Notice = { kind: "ok" | "error" | "info"; text: string };
type ConfigDocument = { yaml: string; revision: string; active_revision: string };

type Props = {
  headers: Record<string, string>;
  notify: (notice: Notice) => void;
  platform: string;
};

function object(value: unknown): ObjectMap {
  return value && typeof value === "object" && !Array.isArray(value) ? value as ObjectMap : {};
}

function text(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function number(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function list(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

async function api<T>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(path, { credentials: "same-origin", ...options });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ message: response.statusText }));
    throw new Error([body.message, ...(body.field_errors ?? [])].filter(Boolean).join("\n"));
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export function HubConnection({ headers, notify, platform }: Props) {
  const [document, setDocument] = useState<ConfigDocument>();
  const [mode, setMode] = useState<"standalone" | "hub">("standalone");
  const [step, setStep] = useState(0);
  const [notifierName, setNotifierName] = useState("backuppo-hub");
  const [url, setUrl] = useState("");
  const [tokenEnv, setTokenEnv] = useState("HUB_TOKEN");
  const [heartbeat, setHeartbeat] = useState(60);
  const [queuePath, setQueuePath] = useState("/var/lib/backuppo/hub-queue.sqlite");
  const [remoteCommands, setRemoteCommands] = useState(false);
  const [pollSeconds, setPollSeconds] = useState(15);
  const [attachJobs, setAttachJobs] = useState(true);
  const [busy, setBusy] = useState(false);

  const parsed = useMemo(() => {
    try { return object(yaml.load(document?.yaml ?? "")); }
    catch { return {}; }
  }, [document]);
  const configuredEntry = useMemo(() => Object.entries(object(parsed.notifiers)).find(([, value]) => text(object(value).type) === "hub"), [parsed]);
  const configured = Boolean(configuredEntry);

  useEffect(() => {
    api<ConfigDocument>("/api/v1/config")
      .then(next => {
        setDocument(next);
        const config = object(yaml.load(next.yaml));
        const entry = Object.entries(object(config.notifiers)).find(([, value]) => text(object(value).type) === "hub");
        if (!entry) return;
        const settings = object(entry[1]);
        setMode("hub");
        setNotifierName(entry[0]);
        setUrl(text(settings.url));
        setTokenEnv(text(settings.token_env, "HUB_TOKEN"));
        setHeartbeat(number(settings.heartbeat_seconds, 60));
        setQueuePath(text(settings.queue_path, "/var/lib/backuppo/hub-queue.sqlite"));
        setRemoteCommands(Boolean(settings.remote_commands));
        setPollSeconds(number(settings.command_poll_seconds, 15));
      })
      .catch(error => notify({ kind: "error", text: error.message }));
  }, [notify]);

  async function persist(config: ObjectMap, success: string): Promise<boolean> {
    if (!document) return false;
    setBusy(true);
    try {
      const source = yaml.dump(config, { noRefs: true, lineWidth: 110, sortKeys: false });
      const saved = await api<{ revision: string }>("/api/v1/config", {
        method: "PUT", headers, body: JSON.stringify({ yaml: source, revision: document.revision })
      });
      await api("/api/v1/config/apply", { method: "POST", headers });
      setDocument({ yaml: source, revision: saved.revision, active_revision: saved.revision });
      notify({ kind: "ok", text: success });
      return true;
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); return false; }
    finally { setBusy(false); }
  }

  async function connect() {
    const config = structuredClone(parsed) as ObjectMap;
    const notifiers = object(config.notifiers);
    const previous = object(notifiers[notifierName]);
    notifiers[notifierName] = {
      ...previous,
      type: "hub",
      url: url.trim().replace(/\/$/, ""),
      token_env: tokenEnv.trim(),
      heartbeat_seconds: heartbeat,
      queue_path: queuePath.trim(),
      remote_commands: remoteCommands,
      command_poll_seconds: pollSeconds,
    };
    config.notifiers = notifiers;
    if (attachJobs) {
      for (const job of Object.values(object(config.jobs))) {
        const settings = object(job);
        const events = object(settings.notify);
        for (const event of ["on_success", "on_failure", "on_verify"]) {
          const names = list(events[event]);
          if (!names.includes(notifierName)) events[event] = [...names, notifierName];
        }
        settings.notify = events;
      }
    }
    if (await persist(config, "Connessione Hub configurata e applicata")) setStep(0);
  }

  async function disconnect() {
    if (!configuredEntry || !window.confirm("Passare alla modalità stand-alone? Il backup locale continuerà a funzionare.")) return;
    const config = structuredClone(parsed) as ObjectMap;
    const notifiers = object(config.notifiers);
    delete notifiers[configuredEntry[0]];
    config.notifiers = notifiers;
    for (const job of Object.values(object(config.jobs))) {
      const events = object(object(job).notify);
      for (const event of ["on_success", "on_failure", "on_verify"]) {
        events[event] = list(events[event]).filter(name => name !== configuredEntry[0]);
      }
    }
    if (Array.isArray(config.reports)) {
      for (const report of config.reports) {
        const settings = object(report);
        settings.notifiers = list(settings.notifiers).filter(name => name !== configuredEntry[0]);
      }
    }
    if (await persist(config, "Modalità stand-alone attivata")) setMode("standalone");
  }

  async function testConnection() {
    if (!configuredEntry) return;
    setBusy(true);
    try {
      await api(`/api/v1/notifiers/${encodeURIComponent(configuredEntry[0])}/test`, { method: "POST", headers });
      notify({ kind: "ok", text: "Connessione riuscita: l’Hub ha accettato l’evento di prova" });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
    finally { setBusy(false); }
  }

  if (!document) return <div className="center"><div className="loader" /><p>Caricamento configurazione…</p></div>;

  const validIdentity = /^https?:\/\//.test(url.trim()) && /^[A-Z_][A-Z0-9_]*$/.test(tokenEnv.trim());
  const secretLocation = platform === "windows"
    ? "nelle variabili d’ambiente del servizio Windows Backuppo"
    : platform === "macos"
      ? "nelle EnvironmentVariables del LaunchDaemon com.backuppo.agent"
      : "in /etc/backuppo/backuppo.env (oppure nel file .env di Docker Compose)";
  return <>
    <section className="connection-hero panel">
      <div><p className="eyebrow">Modalità operativa</p><h2>{configured ? "Agent gestito dall’Hub" : "Agent stand-alone"}</h2><p>{configured ? "Backup locali attivi con supervisione e comandi centralizzati." : "Questo agent lavora in autonomia e conserva localmente configurazione e storico."}</p></div>
      <span className={`connection-state ${configured ? "managed" : "standalone"}`}>{configured ? "● Hub configurato" : "● Stand-alone"}</span>
    </section>

    <div className="mode-selector">
      <button className={mode === "standalone" ? "mode-card selected" : "mode-card"} onClick={() => setMode("standalone")}><strong>Stand-alone</strong><span>Nessun servizio centrale richiesto. Tutte le funzioni di backup restano disponibili.</span></button>
      <button className={mode === "hub" ? "mode-card selected" : "mode-card"} onClick={() => setMode("hub")}><strong>Gestito da Hub</strong><span>Stato centralizzato, policy firmate e comandi remoti opzionali.</span></button>
    </div>

    {mode === "standalone" ? <section className="panel standalone-card">
      <h2>Funzionamento autonomo</h2><p>L’agent continuerà a eseguire job, verifiche e ripristini anche senza Hub.</p>
      {configured ? <button className="danger" disabled={busy} onClick={disconnect}>Disconnetti dall’Hub</button> : <div className="ready-box"><strong>Nessuna dipendenza esterna</strong><span>Non devi configurare altro.</span></div>}
    </section> : <section className="panel hub-connect-card">
      <ol className="hub-steps"><li className={step === 0 ? "active" : step > 0 ? "done" : ""}><span>{step > 0 ? "✓" : "1"}</span>Identità</li><li className={step === 1 ? "active" : step > 1 ? "done" : ""}><span>{step > 1 ? "✓" : "2"}</span>Controllo</li><li className={step === 2 ? "active" : ""}><span>3</span>Conferma</li></ol>
      {step === 0 && <div className="stack"><h2>Collega questo agent</h2><label>URL dell’Hub<input autoFocus placeholder="https://hub.example.com" value={url} onChange={event => setUrl(event.target.value)} /><small>L’indirizzo pubblico raggiungibile da questo server.</small></label><label>Variabile del token<input value={tokenEnv} onChange={event => setTokenEnv(event.target.value.toUpperCase())} /><small>Il token non viene salvato nel YAML. Inseriscilo nell’ambiente del servizio.</small></label><div className="secret-instruction"><span>Imposta il valore {secretLocation}</span><code>{tokenEnv || "HUB_TOKEN"}=token-generato-dall-hub</code></div></div>}
      {step === 1 && <div className="stack"><h2>Comportamento</h2><div className="field-grid"><label>Heartbeat (secondi)<input type="number" min="10" value={heartbeat} onChange={event => setHeartbeat(Number(event.target.value))} /></label><label>Intervallo comandi (secondi)<input type="number" min="5" disabled={!remoteCommands} value={pollSeconds} onChange={event => setPollSeconds(Number(event.target.value))} /></label></div><label>Percorso coda locale<input value={queuePath} onChange={event => setQueuePath(event.target.value)} /><small>Gli eventi restano qui quando l’Hub non è raggiungibile.</small></label><label className="check-row"><input type="checkbox" checked={remoteCommands} onChange={event => setRemoteCommands(event.target.checked)} />Consenti backup e verifiche avviati dall’Hub</label><label className="check-row"><input type="checkbox" checked={attachJobs} onChange={event => setAttachJobs(event.target.checked)} />Invia all’Hub gli esiti di tutti i job</label></div>}
      {step === 2 && <div className="wizard-review"><span>Configurazione</span><strong>{url}</strong><dl><div><dt>Token</dt><dd>Variabile {tokenEnv}</dd></div><div><dt>Heartbeat</dt><dd>Ogni {heartbeat} secondi</dd></div><div><dt>Comandi remoti</dt><dd>{remoteCommands ? "Abilitati" : "Disabilitati"}</dd></div><div><dt>Continuità</dt><dd>Coda locale attiva</dd></div></dl><p>Salvando, i backup locali continueranno a funzionare anche se l’Hub diventa irraggiungibile.</p></div>}
      <div className="wizard-actions"><button className="secondary" disabled={step === 0 || busy} onClick={() => setStep(step - 1)}>Indietro</button>{step < 2 ? <button disabled={busy || (step === 0 ? !validIdentity : heartbeat < 10 || !queuePath.trim())} onClick={() => setStep(step + 1)}>Continua</button> : <button disabled={busy} onClick={connect}>{busy ? "Salvataggio…" : "Salva e collega"}</button>}</div>
      {configured && <div className="connection-tools"><div><strong>Diagnostica</strong><p>Invia un evento firmato con il token presente nell’ambiente del servizio.</p></div><button className="secondary" disabled={busy} onClick={testConnection}>Verifica connessione</button></div>}
    </section>}
  </>;
}
