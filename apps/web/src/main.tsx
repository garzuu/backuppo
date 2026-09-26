import React, { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import yaml from "js-yaml";
import "./styles.css";
import "./guided.css";
import "./recovery.css";
import "./hub.css";
import { ConfigWizard } from "./config-wizard";
import { HubConnection } from "./hub-connection";

type Capability = { mode: "agent" | "hub"; platform: string; product_version: string; features: string[] };
type RecordItem = {
  id: number; job: string; kind: string; status: string; started_at: number;
  duration_ms?: number; bytes?: number; verification_status: string; log: string; error?: string;
};
type Job = { job: string; schedule: string; latest?: RecordItem };
type Operation = { id: string; job: string; kind: string; status: string; detail?: string };
type ConfigDocument = { yaml: string; revision: string; active_revision: string };
type Notice = { kind: "ok" | "error" | "info"; text: string };
type HubSite = { id: number; customer_id: number; name: string; status: string; last_heartbeat_at?: number; last_event_at?: number; agent_version?: string; agent_platform?: string; agent_capabilities: string[] };
type HubCustomer = { id: number; name: string };
type HubExecution = { id: number; site_id: number; job?: string; kind: string; detail?: string; bytes?: number; files?: number; received_at: number };
type HubCommand = { id: number; kind: string; job: string; status: string; detail?: string; created_at: number };
type HubUser = { id: number; username: string; role: string };
type AgentToken = { id: number; site_id: number; created_at: number; revoked_at?: number };
type SignedPolicy = { payload: string; signature: string };
type BackupRef = { job: string; engine: string; id: string; created_at: number; bytes?: number };
type BackupEntry = { path: string; kind: string; bytes?: number };
type UpdateStatus = { current_version: string; channel: string; install_mode: string; target: string; available_version?: string; staged: boolean };
type RuntimeStatus = { policy_blocked: boolean; policy_reason?: string };
type BrowserSession = { authenticated?: boolean; csrf_token?: string; role?: string };

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(path, { credentials: "same-origin", ...options });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ message: response.statusText }));
    throw new Error([body.message, ...(body.field_errors ?? [])].filter(Boolean).join("\n"));
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

function Badge({ value }: { value: string }) {
  return <span className={`badge ${value}`}>{value.replaceAll("_", " ")}</span>;
}

function App() {
  const [capability, setCapability] = useState<Capability>();
  const [session, setSession] = useState<BrowserSession>();
  const [csrf, setCsrf] = useState("");
  const [view, setView] = useState("dashboard");
  const [notice, setNotice] = useState<Notice>();

  useEffect(() => {
    Promise.all([
      request<Capability>("/api/v1/capabilities"),
      request<BrowserSession>("/api/v1/session").catch((): BrowserSession => ({ authenticated: false }))
    ]).then(([nextCapability, session]) => {
      setCapability(nextCapability);
      setSession(session);
      setCsrf(session.csrf_token ?? "");
    }).catch(error => setNotice({ kind: "error", text: error.message }));
  }, []);

  if (!capability || !session) return <main className="center"><div className="loader" /><p>Connessione a Backuppo…</p></main>;

  async function login(username: string, password: string) {
    const nextSession = await request<BrowserSession>("/api/v1/session/login", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username, password })
    });
    setSession(nextSession);
    setCsrf(nextSession.csrf_token ?? csrf);
    setView("dashboard");
    setNotice({ kind: "ok", text: "Accesso effettuato" });
  }

  async function logout() {
    await request("/api/v1/session", { method: "DELETE", headers: mutationHeaders });
    setSession({ authenticated: false, csrf_token: csrf });
    setNotice(undefined);
  }

  if (capability.mode === "hub" && !session.authenticated) {
    return <LoginPage onLogin={login} />;
  }

  const mutationHeaders = { "content-type": "application/json", "x-backuppo-csrf": csrf };
  const navItems = capability.mode === "agent"
    ? [["dashboard", "Panoramica"], ["recovery", "Ripristino"], ["history", "Storico"], ["config", "Configurazione"], ...(capability.features.includes("hub") ? [["hub-connection", "Connessione Hub"]] : []), ...(capability.features.includes("updates") ? [["updates", "Aggiornamenti"]] : [])]
    : [["dashboard", "Panoramica"], ["customers", "Clienti"], ["sites", "Siti"], ["policies", "Policy"], ["users", "Utenti"]];
  const titles: Record<string, string> = { dashboard: "Panoramica", recovery: "Ripristino", history: "Storico esecuzioni", config: "Configurazione", "hub-connection": "Connessione Hub", updates: "Aggiornamenti", customers: "Clienti", sites: "Siti", policies: "Policy di sicurezza", users: "Utenti" };

  return <div className="shell">
    <aside>
      <div className="brand"><img className="logo" src="/backuppo-squirrel-192.png" alt="" /><div><strong>Backuppo</strong><small>{capability.mode} · v{capability.product_version}</small></div></div>
      <nav>
        {navItems.map(([id, label]) =>
          <button key={id} className={view === id ? "active" : ""} onClick={() => setView(id)}>{label}</button>)}
      </nav>
      <p className="aside-note">Restore verificabili, non solo backup completati.</p>
      {capability.mode === "hub" && <button className="secondary sidebar-logout" onClick={() => logout().catch(error => setNotice({ kind: "error", text: error.message }))}>Esci</button>}
    </aside>
    <main>
      <header><div><p className="eyebrow">{capability.mode === "hub" ? "Amministrazione flotta" : "Console operativa"}</p><h1>{titles[view] ?? "Backuppo"}</h1></div><span className="connection">● Connesso</span></header>
      {notice && <div className={`notice ${notice.kind}`}><span>{notice.text}</span><button onClick={() => setNotice(undefined)}>×</button></div>}
      {capability.mode === "agent" && view === "dashboard" && <AgentDashboard headers={mutationHeaders} notify={setNotice} />}
      {capability.mode === "agent" && view === "recovery" && <Recovery headers={mutationHeaders} notify={setNotice} />}
      {capability.mode === "agent" && view === "history" && <History />}
      {capability.mode === "agent" && view === "config" && <ConfigEditor headers={mutationHeaders} notify={setNotice} />}
      {capability.mode === "agent" && view === "hub-connection" && <HubConnection headers={mutationHeaders} notify={setNotice} platform={capability.platform} />}
      {capability.mode === "agent" && view === "updates" && <Updates headers={mutationHeaders} notify={setNotice} />}
      {capability.mode === "hub" && <HubConsole csrf={csrf} notify={setNotice} view={view} role={session.role ?? ""} />}
    </main>
  </div>;
}

function LoginPage({ onLogin }: { onLogin: (username: string, password: string) => Promise<void> }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError("");
    setSubmitting(true);
    try { await onLogin(username, password); }
    catch (loginError) { setError((loginError as Error).message); }
    finally { setSubmitting(false); }
  }

  return <main className="login-page">
    <section className="login-card">
      <div className="login-identity">
        <img src="/backuppo-squirrel-192.png" alt="Scoiattolo Backuppo" />
        <div><p className="eyebrow">Backuppo Hub</p><h1>Bentornato</h1><p>Accedi per gestire clienti, siti e policy di backup.</p></div>
      </div>
      <form onSubmit={submit}>
        <label>Nome utente<input autoFocus required autoComplete="username" value={username} onChange={event => setUsername(event.target.value)} /></label>
        <label>Password<input required type="password" autoComplete="current-password" value={password} onChange={event => setPassword(event.target.value)} /></label>
        {error && <div className="login-error" role="alert">{error}</div>}
        <button disabled={submitting}>{submitting ? "Accesso…" : "Accedi"}</button>
      </form>
      <p className="login-footer">Console di amministrazione Backuppo</p>
    </section>
  </main>;
}

function AgentDashboard({ headers, notify }: { headers: Record<string, string>; notify: (n: Notice) => void }) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [runtime, setRuntime] = useState<RuntimeStatus>();
  const [operations, setOperations] = useState<Record<string, Operation>>({});
  const load = useCallback(() => Promise.all([
    request<Job[]>("/api/v1/status"),
    request<RuntimeStatus>("/api/v1/runtime")
  ]).then(([nextJobs, nextRuntime]) => { setJobs(nextJobs); setRuntime(nextRuntime); }).catch(error => notify({ kind: "error", text: error.message })), [notify]);
  useEffect(() => { load(); const timer = window.setInterval(load, 10_000); return () => clearInterval(timer); }, [load]);

  async function act(job: string, kind: "run" | "verify") {
    try {
      const operation = await request<Operation>(`/api/v1/jobs/${encodeURIComponent(job)}/actions`, { method: "POST", headers, body: JSON.stringify({ kind }) });
      setOperations(previous => ({ ...previous, [job]: operation }));
      const timer = window.setInterval(async () => {
        const current = await request<Operation>(`/api/v1/operations/${operation.id}`);
        setOperations(previous => ({ ...previous, [job]: current }));
        if (["success", "failure"].includes(current.status)) {
          clearInterval(timer); load();
          notify({ kind: current.status === "success" ? "ok" : "error", text: current.detail ?? current.status });
        }
      }, 1500);
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }

  const healthy = jobs.filter(job => job.latest?.status === "success").length;
  return <>
    {runtime?.policy_blocked && <div className="notice error"><span>Policy centrale: {runtime.policy_reason}</span></div>}
    <section className="metrics">
      <article><span>Job configurati</span><strong>{jobs.length}</strong></article>
      <article><span>Ultimo esito positivo</span><strong>{healthy}/{jobs.length}</strong></article>
      <article><span>Verifiche riuscite</span><strong>{jobs.filter(job => job.latest?.verification_status === "success").length}</strong></article>
    </section>
    <section className="panel"><div className="panel-title"><div><h2>Job</h2><p>Backup, schedulazione e verifica restore</p></div><button className="secondary" onClick={load}>Aggiorna</button></div>
      <div className="job-grid">{jobs.map(job => <article className="job-card" key={job.job}>
        <div className="job-head"><div><h3>{job.job}</h3><code>{job.schedule}</code></div><Badge value={operations[job.job]?.status ?? job.latest?.status ?? "never"} /></div>
        <dl><div><dt>Ultima esecuzione</dt><dd>{job.latest ? new Date(job.latest.started_at * 1000).toLocaleString() : "Mai"}</dd></div><div><dt>Verifica restore</dt><dd>{job.latest?.verification_status ?? "non eseguita"}</dd></div></dl>
        <div className="actions"><button onClick={() => act(job.job, "run")}>Esegui backup</button><button className="secondary" onClick={() => act(job.job, "verify")}>Verifica</button></div>
      </article>)}</div>
    </section>
  </>;
}

function Recovery({ headers, notify }: { headers: Record<string, string>; notify: (n: Notice) => void }) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [job, setJob] = useState("");
  const [snapshots, setSnapshots] = useState<BackupRef[]>([]);
  const [snapshot, setSnapshot] = useState("latest");
  const [entries, setEntries] = useState<BackupEntry[]>([]);
  const [target, setTarget] = useState("");
  const [include, setInclude] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => { request<Job[]>("/api/v1/status").then(value => { setJobs(value); setJob(current => current || value[0]?.job || ""); }); }, []);
  useEffect(() => {
    if (!job) return;
    setEntries([]); setSnapshot("latest");
    request<BackupRef[]>(`/api/v1/jobs/${encodeURIComponent(job)}/snapshots`).then(setSnapshots).catch(error => notify({ kind: "error", text: error.message }));
  }, [job, notify]);
  async function browse() {
    try { setEntries(await request<BackupEntry[]>(`/api/v1/jobs/${encodeURIComponent(job)}/browse?snapshot=${encodeURIComponent(snapshot)}`)); }
    catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function restore(dryRun: boolean) {
    if (!target) { notify({ kind: "error", text: "Indica una directory di destinazione." }); return; }
    setBusy(true);
    try {
      const result = await request<{ files: number; bytes: number; dry_run: boolean }>(`/api/v1/jobs/${encodeURIComponent(job)}/restore`, { method: "POST", headers, body: JSON.stringify({ snapshot, target, include: include.split(",").map(value => value.trim()).filter(Boolean), dry_run: dryRun, overwrite: "never" }) });
      notify({ kind: "ok", text: `${result.dry_run ? "Simulazione" : "Ripristino"}: ${result.files} file, ${result.bytes} byte.` });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
    finally { setBusy(false); }
  }
  return <div className="recovery-grid"><section className="panel"><div className="panel-title"><div><h2>Seleziona il backup</h2><p>Consulta gli snapshot senza modificare i dati originali.</p></div></div>
    <div className="stack"><label>Job<select value={job} onChange={event => setJob(event.target.value)}>{jobs.map(item => <option key={item.job}>{item.job}</option>)}</select></label><label>Snapshot<select value={snapshot} onChange={event => setSnapshot(event.target.value)}><option value="latest">Più recente</option>{snapshots.map(item => <option key={item.id} value={item.id}>{new Date(item.created_at * 1000).toLocaleString()} · {item.id.slice(0, 16)}</option>)}</select></label><button className="secondary" disabled={!job} onClick={browse}>Sfoglia contenuto</button></div>
    {entries.length > 0 && <div className="table-wrap recovery-files"><table><thead><tr><th>Tipo</th><th>Percorso</th><th>Byte</th></tr></thead><tbody>{entries.map(entry => <tr key={entry.path}><td>{entry.kind}</td><td><code>{entry.path}</code></td><td>{entry.bytes ?? "—"}</td></tr>)}</tbody></table></div>}</section>
    <section className="panel"><div className="panel-title"><div><h2>Destinazione sicura</h2><p>Per default Backuppo accetta solo directory nuove o vuote.</p></div></div><div className="stack"><label>Directory locale<input placeholder="/srv/restore-test" value={target} onChange={event => setTarget(event.target.value)} /></label><label>Percorsi opzionali<input placeholder="docs, database/export.sql" value={include} onChange={event => setInclude(event.target.value)} /></label><div className="actions"><button className="secondary" disabled={busy || !job} onClick={() => restore(true)}>Simula</button><button disabled={busy || !job} onClick={() => restore(false)}>Ripristina</button></div></div></section></div>;
}

function Updates({ headers, notify }: { headers: Record<string, string>; notify: (n: Notice) => void }) {
  const [status, setStatus] = useState<UpdateStatus>();
  const [busy, setBusy] = useState(false);
  const load = useCallback(() => request<UpdateStatus>("/api/v1/update").then(setStatus).catch(error => notify({ kind: "error", text: error.message })), [notify]);
  useEffect(() => { load(); }, [load]);
  async function action(name: "check" | "download" | "apply" | "rollback") {
    if ((name === "apply" || name === "rollback") && !window.confirm(`${name === "apply" ? "Installare" : "Ripristinare"} il binario e richiedere il riavvio?`)) return;
    setBusy(true);
    try {
      const result = await request<{ status: UpdateStatus; detail: string; restart_required: boolean }>(`/api/v1/update/${name}`, { method: "POST", headers });
      setStatus(result.status); notify({ kind: result.restart_required ? "info" : "ok", text: `${result.detail}${result.restart_required ? ". Riavvio richiesto." : ""}` });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
    finally { setBusy(false); }
  }
  if (!status) return <div className="loader" />;
  return <><section className="metrics"><article><span>Versione installata</span><strong>{status.current_version}</strong></article><article><span>Canale</span><strong>{status.channel}</strong></article><article><span>Disponibile</span><strong>{status.available_version ?? "—"}</strong></article></section><section className="panel"><div className="panel-title"><div><h2>Aggiornamento governato</h2><p>{status.target} · modalità {status.install_mode}. Gli artefatti vengono verificati prima dello staging.</p></div><Badge value={status.staged ? "staged" : "idle"} /></div><div className="actions"><button className="secondary" disabled={busy} onClick={() => action("check")}>Controlla</button><button className="secondary" disabled={busy || !status.available_version} onClick={() => action("download")}>Scarica e verifica</button><button disabled={busy || !status.staged || status.install_mode !== "standalone"} onClick={() => action("apply")}>Installa</button><button className="secondary" disabled={busy || status.install_mode !== "standalone"} onClick={() => action("rollback")}>Rollback</button></div></section></>;
}

function History() {
  const [records, setRecords] = useState<RecordItem[]>([]);
  const [selected, setSelected] = useState<RecordItem>();
  const [query, setQuery] = useState("");
  useEffect(() => { request<RecordItem[]>("/api/v1/runs?limit=200").then(setRecords); }, []);
  const filtered = useMemo(() => records.filter(record => `${record.job} ${record.kind} ${record.status}`.toLowerCase().includes(query.toLowerCase())), [records, query]);
  return <section className="panel"><div className="panel-title"><div><h2>Esecuzioni</h2><p>Seleziona una riga per aprire il log completo</p></div><input placeholder="Cerca job o stato" value={query} onChange={event => setQuery(event.target.value)} /></div>
    <div className="table-wrap"><table><thead><tr><th>Quando</th><th>Job</th><th>Tipo</th><th>Stato</th><th>Durata</th><th>Dimensione</th></tr></thead><tbody>{filtered.map(record => <tr key={record.id} onClick={async () => setSelected(await request<RecordItem>(`/api/v1/runs/${record.id}`))}><td>{new Date(record.started_at * 1000).toLocaleString()}</td><td>{record.job}</td><td>{record.kind}</td><td><Badge value={record.status} /></td><td>{record.duration_ms ? `${(record.duration_ms / 1000).toFixed(1)}s` : "—"}</td><td>{record.bytes ? `${(record.bytes / 1_000_000).toFixed(1)} MB` : "—"}</td></tr>)}</tbody></table></div>
    {selected && <div className="drawer"><div className="panel-title"><div><h2>Log #{selected.id}</h2><p>{selected.job} · {selected.kind}</p></div><button className="secondary" onClick={() => setSelected(undefined)}>Chiudi</button></div><pre>{selected.log}</pre></div>}
  </section>;
}

function ConfigEditor({ headers, notify }: { headers: Record<string, string>; notify: (n: Notice) => void }) {
  const [document, setDocument] = useState<ConfigDocument>();
  const [source, setSource] = useState("");
  const [errors, setErrors] = useState<string[]>([]);
  const [mode, setMode] = useState<"wizard" | "yaml">("wizard");
  const summary = useMemo(() => {
    try {
      const parsed = yaml.load(source) as Record<string, Record<string, unknown>>;
      return { jobs: Object.keys(parsed?.jobs ?? {}).length, destinations: Object.keys(parsed?.destinations ?? {}).length, notifiers: Object.keys(parsed?.notifiers ?? {}).length };
    } catch { return { jobs: 0, destinations: 0, notifiers: 0 }; }
  }, [source]);
  const notifierNames = useMemo(() => {
    try { return Object.keys(((yaml.load(source) as { notifiers?: Record<string, unknown> })?.notifiers) ?? {}); }
    catch { return []; }
  }, [source]);
  const load = useCallback(() => request<ConfigDocument>("/api/v1/config").then(value => { setDocument(value); setSource(value.yaml); }), []);
  useEffect(() => { load(); }, [load]);
  async function validateOnly() { const result = await request<{ valid: boolean; errors: string[] }>("/api/v1/config/validate", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ yaml: source }) }); setErrors(result.errors); notify({ kind: result.valid ? "ok" : "error", text: result.valid ? "Configurazione valida" : "Correggi gli errori evidenziati" }); }
  async function save(applyAfter = false) { try { const result = await request<{ revision: string }>("/api/v1/config", { method: "PUT", headers, body: JSON.stringify({ yaml: source, revision: document?.revision }) }); setDocument(previous => previous && ({ ...previous, yaml: source, revision: result.revision })); setErrors([]); if (applyAfter) await apply(); else notify({ kind: "ok", text: "Configurazione salvata come bozza" }); } catch (error) { notify({ kind: "error", text: (error as Error).message }); } }
  async function apply() { try { const result = await request<{ restart_required: string[] }>("/api/v1/config/apply", { method: "POST", headers }); notify({ kind: "ok", text: result.restart_required.length ? `Applicata; riavvio richiesto per ${result.restart_required.join(", ")}` : "Configurazione applicata al runtime" }); } catch (error) { notify({ kind: "error", text: (error as Error).message }); } }
  async function testNotifier(name: string) { try { await request(`/api/v1/notifiers/${encodeURIComponent(name)}/test`, { method: "POST", headers }); notify({ kind: "ok", text: `Notifier ${name}: test riuscito` }); } catch (error) { notify({ kind: "error", text: (error as Error).message }); } }
  if (!document) return <div className="loader" />;
  return <><section className="metrics"><article><span>Job</span><strong>{summary.jobs}</strong></article><article><span>Destinazioni</span><strong>{summary.destinations}</strong></article><article><span>Notifier</span><strong>{summary.notifiers}</strong></article></section>
    <section className="panel config-panel"><div className="panel-title"><div><h2>Configurazione backup</h2><p>Crea tutti i job necessari. I segreti restano riferimenti a variabili <code>*_env</code>.</p></div><div className="actions"><button className={mode === "wizard" ? "" : "secondary"} onClick={() => setMode("wizard")}>Wizard</button><button className={mode === "yaml" ? "" : "secondary"} onClick={() => setMode("yaml")}>YAML avanzato</button><button className="secondary" onClick={validateOnly}>Valida</button><button className="secondary" onClick={() => save(false)}>Salva bozza</button><button onClick={() => save(true)}>Salva e applica</button></div></div>
      {errors.length > 0 && <ul className="errors">{errors.map(error => <li key={error}>{error}</li>)}</ul>}
      {mode === "yaml" ? <textarea className="editor" spellCheck={false} value={source} onChange={event => setSource(event.target.value)} /> : <ConfigWizard source={source} onChange={setSource} notify={notify} />}
      {notifierNames.length > 0 && <div className="panel-title section-title"><div><h3>Test notifier</h3><p>Invia il messaggio di prova tramite la configurazione attiva.</p></div><div className="actions">{notifierNames.map(name => <button className="secondary" key={name} onClick={() => testNotifier(name)}>{name}</button>)}</div></div>}
    </section></>;
}

function WizardSteps({ labels, step }: { labels: string[]; step: number }) {
  return <ol className="hub-steps">{labels.map((label, index) => <li key={label} className={index === step ? "active" : index < step ? "done" : ""}><span>{index < step ? "✓" : index + 1}</span>{label}</li>)}</ol>;
}

function HubConsole({ csrf, notify, view, role }: { csrf: string; notify: (n: Notice) => void; view: string; role: string }) {
  const [customers, setCustomers] = useState<HubCustomer[]>([]);
  const [sites, setSites] = useState<HubSite[]>([]);
  const [users, setUsers] = useState<HubUser[]>([]);
  const [selected, setSelected] = useState<HubSite>();
  const [jobs, setJobs] = useState<string[]>([]);
  const [executions, setExecutions] = useState<HubExecution[]>([]);
  const [commands, setCommands] = useState<HubCommand[]>([]);
  const [tokens, setTokens] = useState<AgentToken[]>([]);
  const [policies, setPolicies] = useState<SignedPolicy[]>([]);
  const [customerStep, setCustomerStep] = useState(0);
  const [customerName, setCustomerName] = useState("");
  const [siteStep, setSiteStep] = useState(0);
  const [siteName, setSiteName] = useState("");
  const [siteCustomer, setSiteCustomer] = useState("");
  const [issuedToken, setIssuedToken] = useState("");
  const [policyStep, setPolicyStep] = useState(0);
  const [policySite, setPolicySite] = useState("");
  const [policyEnforcement, setPolicyEnforcement] = useState("audit");
  const [policyAppendOnly, setPolicyAppendOnly] = useState(true);
  const [policyObjectLock, setPolicyObjectLock] = useState(true);
  const [policyObjectLockDays, setPolicyObjectLockDays] = useState(30);
  const [policySignedUpdates, setPolicySignedUpdates] = useState(true);
  const [policyRemoteCommands, setPolicyRemoteCommands] = useState(false);
  const [userStep, setUserStep] = useState(0);
  const [newUsername, setNewUsername] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [newRole, setNewRole] = useState("read_only");
  const auth = useMemo(() => ({ "x-backuppo-csrf": csrf, "content-type": "application/json" }), [csrf]);
  const customerNameOf = (id: number) => customers.find(customer => customer.id === id)?.name ?? "Cliente sconosciuto";

  const hubRequest = useCallback(async <T,>(path: string, options: RequestInit = {}): Promise<T> => {
    try { return await request<T>(path, options); }
    catch (firstError) {
      try {
        await request("/api/v1/session/refresh", { method: "POST", headers: auth });
        return await request<T>(path, options);
      } catch { throw firstError; }
    }
  }, [auth]);

  const loadBase = useCallback(async () => {
    try {
      const [nextCustomers, nextSites] = await Promise.all([
        hubRequest<HubCustomer[]>("/v1/customers", { headers: auth }),
        hubRequest<HubSite[]>("/v1/sites", { headers: auth })
      ]);
      setCustomers(nextCustomers); setSites(nextSites);
      if (!siteCustomer && nextCustomers[0]) setSiteCustomer(String(nextCustomers[0].id));
      if (!policySite && nextSites[0]) setPolicySite(String(nextSites[0].id));
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }, [auth, hubRequest, notify, policySite, siteCustomer]);

  useEffect(() => { loadBase(); const timer = window.setInterval(loadBase, 15_000); return () => clearInterval(timer); }, [loadBase]);
  useEffect(() => {
    if (role === "admin") hubRequest<HubUser[]>("/v1/users", { headers: auth }).then(setUsers).catch(() => undefined);
  }, [auth, hubRequest, role]);
  async function openSite(site: HubSite) {
    setSelected(site);
    const [nextJobs, nextExecutions, nextCommands] = await Promise.all([
      hubRequest<string[]>(`/v1/sites/${site.id}/jobs`, { headers: auth }),
      hubRequest<HubExecution[]>(`/v1/sites/${site.id}/executions`, { headers: auth }),
      hubRequest<HubCommand[]>(`/v1/sites/${site.id}/commands`, { headers: auth })
    ]);
    setJobs(nextJobs); setExecutions(nextExecutions); setCommands(nextCommands);
    if (role === "admin") {
      const [nextTokens, nextPolicies] = await Promise.all([
        hubRequest<AgentToken[]>(`/v1/sites/${site.id}/tokens`, { headers: auth }),
        hubRequest<SignedPolicy[]>(`/v1/sites/${site.id}/policies`, { headers: auth })
      ]);
      setTokens(nextTokens); setPolicies(nextPolicies);
    }
  }
  async function command(job: string, kind: "run" | "verify") {
    if (!selected) return;
    try { await hubRequest(`/v1/sites/${selected.id}/commands`, { method: "POST", headers: auth, body: JSON.stringify({ job, kind }) }); await openSite(selected); notify({ kind: "ok", text: `Comando ${kind} accodato per ${job}` }); }
    catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createCustomer() {
    try {
      const customer = await hubRequest<HubCustomer>("/v1/customers", { method: "POST", headers: auth, body: JSON.stringify({ name: customerName }) });
      setCustomers(previous => [...previous, customer]); setCustomerName(""); setCustomerStep(0);
      if (!siteCustomer) setSiteCustomer(String(customer.id));
      notify({ kind: "ok", text: "Cliente creato" });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createSite() {
    try {
      const result = await hubRequest<{ agent_token: string }>("/v1/sites", { method: "POST", headers: auth, body: JSON.stringify({ customer_id: Number(siteCustomer), name: siteName }) });
      setIssuedToken(result.agent_token); setSiteStep(3); await loadBase();
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createUser() {
    try {
      const user = await hubRequest<HubUser>("/v1/users", { method: "POST", headers: auth, body: JSON.stringify({ username: newUsername, password: newPassword, role: newRole }) });
      setUsers(previous => [...previous, user]); setNewUsername(""); setNewPassword(""); setUserStep(0); notify({ kind: "ok", text: "Utente creato" });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createToken() {
    if (!selected) return;
    try {
      const result = await hubRequest<{ agent_token: string }>(`/v1/sites/${selected.id}/tokens`, { method: "POST", headers: auth });
      setTokens(await hubRequest<AgentToken[]>(`/v1/sites/${selected.id}/tokens`, { headers: auth }));
      notify({ kind: "info", text: `Token agent (mostrato una sola volta):\n${result.agent_token}` });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function revokeToken(id: number) {
    if (!selected) return;
    try { await hubRequest(`/v1/sites/${selected.id}/tokens/${id}`, { method: "DELETE", headers: auth }); setTokens(await hubRequest<AgentToken[]>(`/v1/sites/${selected.id}/tokens`, { headers: auth })); }
    catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createPolicy() {
    const siteId = Number(policySite);
    if (!siteId) return;
    try {
      await hubRequest(`/v1/sites/${siteId}/policies`, { method: "POST", headers: auth, body: JSON.stringify({
        expires_at: Math.floor(Date.now() / 1000) + 90 * 24 * 3600,
        enforcement: policyEnforcement,
        constraints: { require_append_only: policyAppendOnly, require_object_lock: policyObjectLock, minimum_object_lock_days: policyObjectLock ? policyObjectLockDays : null, require_signed_updates: policySignedUpdates, allow_remote_commands: policyRemoteCommands }
      }) });
      setPolicies(await hubRequest<SignedPolicy[]>(`/v1/sites/${siteId}/policies`, { headers: auth }));
      setPolicyStep(0); notify({ kind: "ok", text: "Policy firmata e pubblicata" });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }

  const wizardButtons = (step: number, setStep: (value: number) => void, max: number, valid: boolean, finish: () => void, finishLabel: string) =>
    <div className="wizard-actions"><button className="secondary" disabled={step === 0} onClick={() => setStep(step - 1)}>Indietro</button>{step < max ? <button disabled={!valid} onClick={() => setStep(step + 1)}>Continua</button> : <button disabled={!valid} onClick={finish}>{finishLabel}</button>}</div>;

  if (view === "dashboard") return <>
    <section className="metrics"><article><span>Clienti</span><strong>{customers.length}</strong></article><article><span>Siti online</span><strong>{sites.filter(site => site.status === "online").length}/{sites.length}</strong></article><article><span>Siti da verificare</span><strong>{sites.filter(site => site.status !== "online").length}</strong></article></section>
    <section className="panel"><div className="panel-title"><div><h2>Stato della flotta</h2><p>Una vista rapida; usa la sidebar per configurare ogni area.</p></div></div><div className="fleet-list">{sites.map(site => <article key={site.id}><div><strong>{site.name}</strong><small>{customerNameOf(site.customer_id)} · {site.agent_platform ?? "agent non connesso"}</small></div><Badge value={site.status} /></article>)}{sites.length === 0 && <div className="empty-state"><h3>Nessun sito configurato</h3><p>Crea prima un cliente, poi registra il suo primo sito.</p></div>}</div></section>
  </>;

  if (view === "customers") return <div className="admin-layout"><section className="panel"><div className="panel-title"><div><h2>Clienti</h2><p>Organizzazioni gestite dall’hub.</p></div><span className="count-pill">{customers.length}</span></div><div className="entity-list">{customers.map(customer => <article key={customer.id}><div className="entity-icon">C</div><div><strong>{customer.name}</strong><small>{sites.filter(site => site.customer_id === customer.id).length} siti</small></div></article>)}</div></section>{role === "admin" && <section className="panel wizard-card"><h2>Nuovo cliente</h2><WizardSteps labels={["Dati", "Riepilogo"]} step={customerStep} />{customerStep === 0 ? <label>Nome del cliente<input autoFocus placeholder="Acme S.r.l." value={customerName} onChange={event => setCustomerName(event.target.value)} /></label> : <div className="wizard-review"><span>Cliente</span><strong>{customerName}</strong><p>Potrai aggiungere uno o più siti subito dopo.</p></div>}{wizardButtons(customerStep, setCustomerStep, 1, customerName.trim().length > 1, createCustomer, "Crea cliente")}</section>}</div>;

  if (view === "sites") return <><div className="admin-layout"><section className="panel"><div className="panel-title"><div><h2>Siti</h2><p>Seleziona un sito per aprire il dettaglio operativo.</p></div><span className="count-pill">{sites.length}</span></div><div className="site-list">{sites.map(site => <button key={site.id} className={selected?.id === site.id ? "selected" : ""} onClick={() => openSite(site)}><span><strong>{site.name}</strong><small>{customerNameOf(site.customer_id)}</small></span><Badge value={site.status} /></button>)}</div></section>{role === "admin" && <section className="panel wizard-card"><h2>Registra un sito</h2><WizardSteps labels={["Cliente", "Sito", "Conferma", "Token"]} step={siteStep} />{siteStep === 0 && <label>Cliente<select value={siteCustomer} onChange={event => setSiteCustomer(event.target.value)}><option value="">Seleziona un cliente</option>{customers.map(customer => <option key={customer.id} value={customer.id}>{customer.name}</option>)}</select></label>}{siteStep === 1 && <label>Nome del sito<input autoFocus placeholder="Milano · Sede principale" value={siteName} onChange={event => setSiteName(event.target.value)} /></label>}{siteStep === 2 && <div className="wizard-review"><span>Registrazione</span><strong>{siteName}</strong><p>{customers.find(customer => String(customer.id) === siteCustomer)?.name}</p></div>}{siteStep === 3 && <div className="token-result"><strong>Salva ora il token agent</strong><code>{issuedToken}</code><p>Non sarà più visualizzabile.</p><button onClick={() => { setSiteStep(0); setSiteName(""); setIssuedToken(""); }}>Registra un altro sito</button></div>}{siteStep < 3 && wizardButtons(siteStep, setSiteStep, 2, siteStep === 0 ? Boolean(siteCustomer) : siteName.trim().length > 1, createSite, "Crea sito")}</section>}</div>
    {selected && <section className="panel site-detail"><div className="panel-title"><div><p className="eyebrow">{customerNameOf(selected.customer_id)}</p><h2>{selected.name}</h2><p>Agent {selected.agent_version ?? "non connesso"} · {selected.agent_platform ?? "piattaforma sconosciuta"}</p></div><Badge value={selected.status} /></div><h3>Job</h3><div className="job-grid">{jobs.map(job => <article className="job-card" key={job}><h3>{job}</h3><div className="actions"><button onClick={() => command(job, "run")}>Esegui</button><button className="secondary" onClick={() => command(job, "verify")}>Verifica</button></div></article>)}{jobs.length === 0 && <p className="muted">Nessun job ancora riportato dall’agent.</p>}</div><h3 className="section-title">Esecuzioni recenti</h3><div className="table-wrap"><table><thead><tr><th>Quando</th><th>Job</th><th>Evento</th><th>Dettaglio</th></tr></thead><tbody>{executions.map(item => <tr key={item.id}><td>{new Date(item.received_at * 1000).toLocaleString()}</td><td>{item.job ?? "—"}</td><td><Badge value={item.kind} /></td><td>{item.detail ?? "—"}</td></tr>)}</tbody></table></div>{role === "admin" && <><div className="panel-title section-title"><div><h3>Token agent</h3><p>Credenziali di collegamento del sito.</p></div><button onClick={createToken}>Nuovo token</button></div><div className="table-wrap"><table><thead><tr><th>Creato</th><th>Stato</th><th></th></tr></thead><tbody>{tokens.map(item => <tr key={item.id}><td>{new Date(item.created_at * 1000).toLocaleString()}</td><td>{item.revoked_at ? "Revocato" : "Attivo"}</td><td>{!item.revoked_at && <button className="secondary" onClick={() => revokeToken(item.id)}>Revoca</button>}</td></tr>)}</tbody></table></div></>}</section>}</>;

  if (view === "policies") return <section className="panel wizard-card wide"><div className="panel-title"><div><h2>Nuova policy firmata</h2><p>Definisci i requisiti, controlla il riepilogo e pubblica.</p></div><span className="count-pill">{policies.length} versioni</span></div><WizardSteps labels={["Sito", "Requisiti", "Applicazione"]} step={policyStep} />{policyStep === 0 && <label>Sito destinatario<select value={policySite} onChange={event => setPolicySite(event.target.value)}><option value="">Seleziona un sito</option>{sites.map(site => <option key={site.id} value={site.id}>{customerNameOf(site.customer_id)} · {site.name}</option>)}</select></label>}{policyStep === 1 && <div className="policy-options"><label className="check-row"><input type="checkbox" checked={policyAppendOnly} onChange={event => setPolicyAppendOnly(event.target.checked)} />Repository append-only obbligatorio</label><label className="check-row"><input type="checkbox" checked={policyObjectLock} onChange={event => setPolicyObjectLock(event.target.checked)} />S3 Object Lock obbligatorio</label>{policyObjectLock && <label>Retention minima in giorni<input type="number" min="1" value={policyObjectLockDays} onChange={event => setPolicyObjectLockDays(Number(event.target.value))} /></label>}<label className="check-row"><input type="checkbox" checked={policySignedUpdates} onChange={event => setPolicySignedUpdates(event.target.checked)} />Aggiornamenti firmati obbligatori</label><label className="check-row"><input type="checkbox" checked={policyRemoteCommands} onChange={event => setPolicyRemoteCommands(event.target.checked)} />Consenti comandi remoti</label></div>}{policyStep === 2 && <div className="wizard-review"><label>Comportamento<select value={policyEnforcement} onChange={event => setPolicyEnforcement(event.target.value)}><option value="audit">Segnala soltanto</option><option value="block">Blocca agent non conformi</option></select></label><dl><div><dt>Sito</dt><dd>{sites.find(site => String(site.id) === policySite)?.name}</dd></div><div><dt>Immutabilità</dt><dd>{policyAppendOnly ? "Richiesta" : "Non richiesta"}</dd></div><div><dt>Object Lock</dt><dd>{policyObjectLock ? `${policyObjectLockDays} giorni` : "Non richiesto"}</dd></div><div><dt>Validità</dt><dd>90 giorni</dd></div></dl></div>}{wizardButtons(policyStep, setPolicyStep, 2, policyStep === 0 ? Boolean(policySite) : !policyObjectLock || policyObjectLockDays > 0, createPolicy, "Firma e pubblica")}</section>;

  if (view === "users") return <div className="admin-layout"><section className="panel"><div className="panel-title"><div><h2>Utenti</h2><p>Accessi alla console e relativi ruoli.</p></div><span className="count-pill">{users.length}</span></div><div className="entity-list">{users.map(user => <article key={user.id}><div className="entity-icon">U</div><div><strong>{user.username}</strong><small>{user.role.replaceAll("_", " ")}</small></div></article>)}</div></section>{role === "admin" && <section className="panel wizard-card"><h2>Nuovo utente</h2><WizardSteps labels={["Credenziali", "Ruolo", "Conferma"]} step={userStep} />{userStep === 0 && <div className="stack"><label>Nome utente<input value={newUsername} onChange={event => setNewUsername(event.target.value)} /></label><label>Password iniziale<input type="password" value={newPassword} onChange={event => setNewPassword(event.target.value)} /></label></div>}{userStep === 1 && <label>Ruolo<select value={newRole} onChange={event => setNewRole(event.target.value)}><option value="read_only">Sola lettura</option><option value="operator">Operatore</option><option value="admin">Amministratore</option></select></label>}{userStep === 2 && <div className="wizard-review"><span>Nuovo accesso</span><strong>{newUsername}</strong><p>Ruolo: {newRole.replaceAll("_", " ")}</p></div>}{wizardButtons(userStep, setUserStep, 2, newUsername.trim().length > 1 && newPassword.length >= 8, createUser, "Crea utente")}</section>}</div>;

  return <section className="panel"><p>Sezione non disponibile.</p></section>;
}

createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);

if ("serviceWorker" in navigator && import.meta.env.PROD) {
  window.addEventListener("load", () => navigator.serviceWorker.register("/sw.js"));
}
