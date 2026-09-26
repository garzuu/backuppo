import React, { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import yaml from "js-yaml";
import "./styles.css";
import "./guided.css";

type Capability = { mode: "agent" | "hub"; product_version: string; features: string[] };
type RecordItem = {
  id: number; job: string; kind: string; status: string; started_at: number;
  duration_ms?: number; bytes?: number; verification_status: string; log: string; error?: string;
};
type Job = { job: string; schedule: string; latest?: RecordItem };
type Operation = { id: string; job: string; kind: string; status: string; detail?: string };
type ConfigDocument = { yaml: string; revision: string; active_revision: string };
type Notice = { kind: "ok" | "error" | "info"; text: string };
type HubSite = { id: number; customer_id: number; name: string; status: string; last_heartbeat_at?: number; last_event_at?: number };
type HubExecution = { id: number; site_id: number; job?: string; kind: string; detail?: string; bytes?: number; files?: number; received_at: number };
type HubCommand = { id: number; kind: string; job: string; status: string; detail?: string; created_at: number };
type HubUser = { id: number; username: string; role: string };
type AgentToken = { id: number; site_id: number; created_at: number; revoked_at?: number };

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
  const [csrf, setCsrf] = useState("");
  const [view, setView] = useState("dashboard");
  const [notice, setNotice] = useState<Notice>();

  useEffect(() => {
    Promise.all([
      request<Capability>("/api/v1/capabilities"),
      request<{ csrf_token?: string }>("/api/v1/session").catch((): { csrf_token?: string } => ({}))
    ]).then(([nextCapability, session]) => {
      setCapability(nextCapability);
      setCsrf(session.csrf_token ?? "");
    }).catch(error => setNotice({ kind: "error", text: error.message }));
  }, []);

  if (!capability) return <main className="center"><div className="loader" /><p>Connessione a Backuppo…</p></main>;
  const mutationHeaders = { "content-type": "application/json", "x-backuppo-csrf": csrf };

  return <div className="shell">
    <aside>
      <div className="brand"><span className="logo">B</span><div><strong>Backuppo</strong><small>{capability.mode} · v{capability.product_version}</small></div></div>
      <nav>
        {(capability.mode === "agent" ? [["dashboard", "Panoramica"], ["history", "Storico"], ["config", "Configurazione"]] : [["dashboard", "Siti e clienti"]]).map(([id, label]) =>
          <button key={id} className={view === id ? "active" : ""} onClick={() => setView(id)}>{label}</button>)}
      </nav>
      <p className="aside-note">Restore verificabili, non solo backup completati.</p>
    </aside>
    <main>
      <header><div><p className="eyebrow">Console operativa</p><h1>{view === "dashboard" ? "Panoramica" : view === "history" ? "Storico esecuzioni" : "Configurazione"}</h1></div><span className="connection">● Connesso</span></header>
      {notice && <div className={`notice ${notice.kind}`}><span>{notice.text}</span><button onClick={() => setNotice(undefined)}>×</button></div>}
      {capability.mode === "agent" && view === "dashboard" && <AgentDashboard headers={mutationHeaders} notify={setNotice} />}
      {capability.mode === "agent" && view === "history" && <History />}
      {capability.mode === "agent" && view === "config" && <ConfigEditor headers={mutationHeaders} notify={setNotice} />}
      {capability.mode === "hub" && <HubConsole csrf={csrf} notify={setNotice} />}
    </main>
  </div>;
}

function AgentDashboard({ headers, notify }: { headers: Record<string, string>; notify: (n: Notice) => void }) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [operations, setOperations] = useState<Record<string, Operation>>({});
  const load = useCallback(() => request<Job[]>("/api/v1/status").then(setJobs).catch(error => notify({ kind: "error", text: error.message })), [notify]);
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
  const [mode, setMode] = useState<"guided" | "yaml">("guided");
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
  async function save() { try { const result = await request<{ revision: string }>("/api/v1/config", { method: "PUT", headers, body: JSON.stringify({ yaml: source, revision: document?.revision }) }); setDocument(previous => previous && ({ ...previous, yaml: source, revision: result.revision })); setErrors([]); notify({ kind: "ok", text: "Configurazione salvata; ora puoi applicarla" }); } catch (error) { notify({ kind: "error", text: (error as Error).message }); } }
  async function apply() { try { const result = await request<{ restart_required: string[] }>("/api/v1/config/apply", { method: "POST", headers }); notify({ kind: "ok", text: result.restart_required.length ? `Applicata; riavvio richiesto per ${result.restart_required.join(", ")}` : "Configurazione applicata al runtime" }); } catch (error) { notify({ kind: "error", text: (error as Error).message }); } }
  async function testNotifier(name: string) { try { await request(`/api/v1/notifiers/${encodeURIComponent(name)}/test`, { method: "POST", headers }); notify({ kind: "ok", text: `Notifier ${name}: test riuscito` }); } catch (error) { notify({ kind: "error", text: (error as Error).message }); } }
  if (!document) return <div className="loader" />;
  return <><section className="metrics"><article><span>Job</span><strong>{summary.jobs}</strong></article><article><span>Destinazioni</span><strong>{summary.destinations}</strong></article><article><span>Notifier</span><strong>{summary.notifiers}</strong></article></section>
    <section className="panel"><div className="panel-title"><div><h2>Configurazione</h2><p>I valori segreti restano riferimenti a variabili <code>*_env</code>.</p></div><div className="actions"><button className={mode === "guided" ? "" : "secondary"} onClick={() => setMode("guided")}>Form guidato</button><button className={mode === "yaml" ? "" : "secondary"} onClick={() => setMode("yaml")}>YAML</button><button className="secondary" onClick={validateOnly}>Valida</button><button className="secondary" onClick={save}>Salva</button><button onClick={apply}>Applica</button></div></div>
      {errors.length > 0 && <ul className="errors">{errors.map(error => <li key={error}>{error}</li>)}</ul>}
      {mode === "yaml" ? <textarea className="editor" spellCheck={false} value={source} onChange={event => setSource(event.target.value)} /> : <GuidedConfig source={source} onChange={setSource} />}
      {notifierNames.length > 0 && <div className="panel-title section-title"><div><h3>Test notifier</h3><p>Invia il messaggio di prova tramite la configurazione attiva.</p></div><div className="actions">{notifierNames.map(name => <button className="secondary" key={name} onClick={() => testNotifier(name)}>{name}</button>)}</div></div>}
    </section></>;
}

function GuidedConfig({ source, onChange }: { source: string; onChange: (value: string) => void }) {
  let parsed: Record<string, unknown>;
  try { parsed = (yaml.load(source) as Record<string, unknown>) ?? {}; }
  catch { return <p className="errors">Il YAML non è analizzabile: passa alla modalità YAML per correggerlo.</p>; }

  function update(path: string[], raw: string | boolean) {
    const copy = structuredClone(parsed) as Record<string, unknown>;
    let cursor: Record<string, unknown> = copy;
    path.slice(0, -1).forEach(key => { cursor = cursor[key] as Record<string, unknown>; });
    const key = path[path.length - 1];
    const previous = cursor[key];
    cursor[key] = typeof previous === "number" ? Number(raw) : Array.isArray(previous) ? String(raw).split(",").map(value => value.trim()).filter(Boolean) : raw;
    onChange(yaml.dump(copy, { noRefs: true, lineWidth: 110 }));
  }

  function fields(value: unknown, path: string[]): React.ReactNode {
    if (value && typeof value === "object" && !Array.isArray(value)) return <div className="form-group">{Object.entries(value as Record<string, unknown>).map(([key, child]) => <div key={key} className="form-row"><label>{key}</label>{fields(child, [...path, key])}</div>)}</div>;
    if (typeof value === "boolean") return <input type="checkbox" checked={value} onChange={event => update(path, event.target.checked)} />;
    return <input value={Array.isArray(value) ? value.join(", ") : String(value ?? "")} onChange={event => update(path, event.target.value)} />;
  }

  const sections = ["jobs", "destinations", "notifiers", "reports", "observability", "api"];
  return <div className="guided-grid">{sections.filter(section => parsed[section] !== undefined).map(section => <details key={section} open={section === "jobs"}><summary>{section}</summary>{fields(parsed[section], [section])}</details>)}</div>;
}

function HubConsole({ csrf, notify }: { csrf: string; notify: (n: Notice) => void }) {
  const [authenticated, setAuthenticated] = useState<boolean | undefined>();
  const [role, setRole] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [sites, setSites] = useState<HubSite[]>([]);
  const [selected, setSelected] = useState<HubSite>();
  const [jobs, setJobs] = useState<string[]>([]);
  const [executions, setExecutions] = useState<HubExecution[]>([]);
  const [commands, setCommands] = useState<HubCommand[]>([]);
  const [users, setUsers] = useState<HubUser[]>([]);
  const [tokens, setTokens] = useState<AgentToken[]>([]);
  const [customerName, setCustomerName] = useState("");
  const [siteName, setSiteName] = useState("");
  const [customerId, setCustomerId] = useState("");
  const [newUsername, setNewUsername] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [newRole, setNewRole] = useState("read_only");
  const auth = useMemo(() => ({ "x-backuppo-csrf": csrf, "content-type": "application/json" }), [csrf]);

  const hubRequest = useCallback(async <T,>(path: string, options: RequestInit = {}): Promise<T> => {
    try { return await request<T>(path, options); }
    catch (firstError) {
      try {
        await request("/api/v1/session/refresh", { method: "POST", headers: auth });
        return await request<T>(path, options);
      } catch { throw firstError; }
    }
  }, [auth]);

  useEffect(() => {
    request<{ authenticated: boolean; role?: string }>("/api/v1/session")
      .then(session => { setAuthenticated(session.authenticated); setRole(session.role ?? ""); })
      .catch(() => setAuthenticated(false));
  }, []);
  useEffect(() => {
    if (authenticated && role === "admin") hubRequest<HubUser[]>("/v1/users", { headers: auth }).then(setUsers).catch(() => undefined);
  }, [auth, authenticated, hubRequest, role]);

  const loadSites = useCallback(async () => {
    if (!authenticated) return;
    try { setSites(await hubRequest<HubSite[]>("/v1/sites", { headers: auth })); }
    catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }, [auth, authenticated, hubRequest, notify]);
  useEffect(() => { loadSites(); const timer = window.setInterval(loadSites, 15_000); return () => clearInterval(timer); }, [loadSites]);

  async function login(event: React.FormEvent) {
    event.preventDefault();
    try {
      const session = await request<{ role?: string }>("/api/v1/session/login", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ username, password }) });
      setAuthenticated(true); setRole(session.role ?? ""); setPassword(""); notify({ kind: "ok", text: "Accesso effettuato" });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function openSite(site: HubSite) {
    setSelected(site);
    const [nextJobs, nextExecutions, nextCommands] = await Promise.all([
      hubRequest<string[]>(`/v1/sites/${site.id}/jobs`, { headers: auth }),
      hubRequest<HubExecution[]>(`/v1/sites/${site.id}/executions`, { headers: auth }),
      hubRequest<HubCommand[]>(`/v1/sites/${site.id}/commands`, { headers: auth })
    ]);
    setJobs(nextJobs); setExecutions(nextExecutions); setCommands(nextCommands);
    if (role === "admin") setTokens(await hubRequest<AgentToken[]>(`/v1/sites/${site.id}/tokens`, { headers: auth }));
  }
  async function command(job: string, kind: "run" | "verify") {
    if (!selected) return;
    try { await hubRequest(`/v1/sites/${selected.id}/commands`, { method: "POST", headers: auth, body: JSON.stringify({ job, kind }) }); await openSite(selected); notify({ kind: "ok", text: `Comando ${kind} accodato per ${job}` }); }
    catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createCustomer() {
    try { await hubRequest("/v1/customers", { method: "POST", headers: auth, body: JSON.stringify({ name: customerName }) }); setCustomerName(""); notify({ kind: "ok", text: "Cliente creato" }); }
    catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createSite() {
    try {
      const result = await hubRequest<{ agent_token: string }>("/v1/sites", { method: "POST", headers: auth, body: JSON.stringify({ customer_id: Number(customerId), name: siteName }) });
      setSiteName(""); await loadSites(); notify({ kind: "info", text: `Token agent (mostrato una sola volta):\n${result.agent_token}` });
    } catch (error) { notify({ kind: "error", text: (error as Error).message }); }
  }
  async function createUser() {
    try {
      const user = await hubRequest<HubUser>("/v1/users", { method: "POST", headers: auth, body: JSON.stringify({ username: newUsername, password: newPassword, role: newRole }) });
      setUsers(previous => [...previous, user]); setNewUsername(""); setNewPassword(""); notify({ kind: "ok", text: "Utente creato" });
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

  if (authenticated === undefined) return <div className="loader" />;
  if (!authenticated) return <section className="panel login-panel"><div><p className="eyebrow">Backuppo Hub</p><h2>Accedi alla console</h2><p className="muted">Usa un account admin, operator o read-only.</p></div><form onSubmit={login}><label>Utente<input autoComplete="username" value={username} onChange={event => setUsername(event.target.value)} /></label><label>Password<input type="password" autoComplete="current-password" value={password} onChange={event => setPassword(event.target.value)} /></label><button>Accedi</button></form></section>;

  return <>
    <section className="metrics"><article><span>Siti</span><strong>{sites.length}</strong></article><article><span>Online</span><strong>{sites.filter(site => site.status === "online").length}</strong></article><article><span>Offline</span><strong>{sites.filter(site => site.status === "offline").length}</strong></article></section>
    <div className="hub-grid"><section className="panel"><div className="panel-title"><div><h2>Siti</h2><p>Seleziona un sito per job, eventi e comandi</p></div><button className="secondary" onClick={async () => { await request("/api/v1/session", { method: "DELETE", headers: auth }); setAuthenticated(false); }}>Esci</button></div><div className="site-list">{sites.map(site => <button key={site.id} className={selected?.id === site.id ? "selected" : ""} onClick={() => openSite(site)}><span><strong>{site.name}</strong><small>Cliente #{site.customer_id}</small></span><Badge value={site.status} /></button>)}</div></section>
      {role === "admin" && <section className="panel"><h2>Amministrazione</h2><div className="stack"><label>Nuovo cliente<input placeholder="Nome cliente" value={customerName} onChange={event => setCustomerName(event.target.value)} /></label><button onClick={createCustomer}>Crea cliente</button><label>ID cliente<input inputMode="numeric" placeholder="1" value={customerId} onChange={event => setCustomerId(event.target.value)} /></label><label>Nuovo sito<input placeholder="Nome sito" value={siteName} onChange={event => setSiteName(event.target.value)} /></label><button onClick={createSite}>Crea sito e token</button><hr/><label>Nuovo utente<input value={newUsername} onChange={event => setNewUsername(event.target.value)} /></label><label>Password<input type="password" value={newPassword} onChange={event => setNewPassword(event.target.value)} /></label><label>Ruolo<select value={newRole} onChange={event => setNewRole(event.target.value)}><option value="read_only">Sola lettura</option><option value="operator">Operator</option><option value="admin">Admin</option></select></label><button onClick={createUser}>Crea utente</button><small className="muted">{users.length} utenti configurati</small></div></section>}</div>
    {selected && <section className="panel site-detail"><div className="panel-title"><div><p className="eyebrow">Sito #{selected.id}</p><h2>{selected.name}</h2></div><Badge value={selected.status} /></div>
      <h3>Job</h3><div className="job-grid">{jobs.map(job => <article className="job-card" key={job}><h3>{job}</h3><div className="actions"><button onClick={() => command(job, "run")}>Esegui</button><button className="secondary" onClick={() => command(job, "verify")}>Verifica</button></div></article>)}</div>
      <h3 className="section-title">Esecuzioni recenti</h3><div className="table-wrap"><table><thead><tr><th>Quando</th><th>Job</th><th>Evento</th><th>Dettaglio</th></tr></thead><tbody>{executions.map(item => <tr key={item.id}><td>{new Date(item.received_at * 1000).toLocaleString()}</td><td>{item.job ?? "—"}</td><td><Badge value={item.kind} /></td><td>{item.detail ?? "—"}</td></tr>)}</tbody></table></div>
      <h3 className="section-title">Comandi</h3><div className="table-wrap"><table><thead><tr><th>Quando</th><th>Job</th><th>Azione</th><th>Stato</th></tr></thead><tbody>{commands.map(item => <tr key={item.id}><td>{new Date(item.created_at * 1000).toLocaleString()}</td><td>{item.job}</td><td>{item.kind}</td><td><Badge value={item.status} /></td></tr>)}</tbody></table></div>
      {role === "admin" && <><div className="panel-title section-title"><div><h3>Token agent</h3><p>I valori segreti non sono recuperabili dopo la creazione.</p></div><button onClick={createToken}>Nuovo token</button></div><div className="table-wrap"><table><thead><tr><th>ID</th><th>Creato</th><th>Stato</th><th></th></tr></thead><tbody>{tokens.map(item => <tr key={item.id}><td>#{item.id}</td><td>{new Date(item.created_at * 1000).toLocaleString()}</td><td>{item.revoked_at ? "revocato" : "attivo"}</td><td>{!item.revoked_at && <button className="secondary" onClick={() => revokeToken(item.id)}>Revoca</button>}</td></tr>)}</tbody></table></div></>}
    </section>}
  </>;
}

createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);

if ("serviceWorker" in navigator && import.meta.env.PROD) {
  window.addEventListener("load", () => navigator.serviceWorker.register("/sw.js"));
}
