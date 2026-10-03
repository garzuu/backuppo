import React, { useEffect, useMemo, useState } from "react";
import yaml from "js-yaml";

type ObjectMap = Record<string, unknown>;
type WizardProps = {
  source: string;
  onChange: (value: string) => void;
  notify: (notice: { kind: "ok" | "error" | "info"; text: string }) => void;
};

const STEPS = ["Sorgente", "Destinazione", "Pianificazione", "Protezione", "Riepilogo"];
const SCHEDULES: Record<string, string> = {
  "0 2 * * *": "Ogni notte alle 02:00",
  "0 3 * * *": "Ogni notte alle 03:00",
  "0 */6 * * *": "Ogni 6 ore",
  "0 3 * * 0": "Ogni domenica alle 03:00",
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

function optionalNumber(value: unknown): number | "" {
  return typeof value === "number" && Number.isFinite(value) ? value : "";
}

function list(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function dump(config: ObjectMap): string {
  return yaml.dump(config, { noRefs: true, lineWidth: 110, sortKeys: false });
}

function uniqueName(base: string, existing: string[]): string {
  if (!existing.includes(base)) return base;
  let suffix = 2;
  while (existing.includes(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function sourceDefaults(type: string): ObjectMap {
  switch (type) {
    case "postgres": return { type, host: "127.0.0.1", port: 5432, user: "postgres", password_env: "POSTGRES_PASSWORD", database: "postgres" };
    case "mysql": return { type, host: "127.0.0.1", port: 3306, user: "root", password_env: "MYSQL_PASSWORD", database: "mysql" };
    case "sqlite": return { type, path: "/srv/data/app.sqlite" };
    case "docker_volume": return { type, volume: "nome-volume" };
    case "command": return { type, command: "/usr/local/bin/export-data", args: [], output_filename: "output" };
    case "disk_image": return { type, path: "/dev/disk/by-id/…", output_filename: "disk.img" };
    case "libvirt_vm": return { type, name: "nome-vm", virsh_binary: "virsh" };
    case "proxmox_vm": return { type, vmid: 100, mode: "snapshot", vzdump_binary: "vzdump" };
    default: return { type: "folder", path: "/srv/data", exclude: [] };
  }
}

function notifierDefaults(type: string): ObjectMap {
  switch (type) {
    case "telegram": return { type, token_env: "TELEGRAM_BOT_TOKEN", chat_id: "" };
    case "smtp": return { type, host: "smtp.example.com", port: 587, user: "", password_env: "SMTP_PASSWORD", from: "", to: [] };
    case "ntfy": return { type, url: "https://ntfy.sh", topic: "" };
    default: return { type: "webhook", url: "" };
  }
}

function destinationDefaults(type: string): ObjectMap {
  switch (type) {
    case "sftp": return { type, host: "backup.example.com", port: 22, user: "backup", key_path: "/root/.ssh/id_ed25519", root: "/backups", retry: { max_times: 3 } };
    case "s3": return { type, bucket: "backups", region: "eu-south-1", access_key_id_env: "S3_ACCESS_KEY_ID", secret_access_key_env: "S3_SECRET_ACCESS_KEY", root: "", retry: { max_times: 3 } };
    case "webdav": return { type, url: "https://cloud.example.com/remote.php/dav/files/backup", user: "backup", password_env: "WEBDAV_PASSWORD", retry: { max_times: 3 } };
    case "google_drive": return { type, root: "backuppo", access_token_env: "GOOGLE_DRIVE_ACCESS_TOKEN", refresh_token_env: "GOOGLE_DRIVE_REFRESH_TOKEN", retry: { max_times: 3 } };
    case "dropbox": return { type, root: "backuppo", access_token_env: "DROPBOX_ACCESS_TOKEN", refresh_token_env: "DROPBOX_REFRESH_TOKEN", retry: { max_times: 3 } };
    case "one_drive": return { type, root: "backuppo", access_token_env: "ONEDRIVE_ACCESS_TOKEN", refresh_token_env: "ONEDRIVE_REFRESH_TOKEN", retry: { max_times: 3 } };
    case "restic": return { type, repository: "/var/lib/backuppo/restic", password_env: "RESTIC_PASSWORD", environment: {}, maintenance_environment: {}, binary: "restic", initialize: true, append_only: false };
    default: return { type: "fs", root: "/var/lib/backuppo/backups" };
  }
}

function Field({ label, hint, value, type = "text", placeholder, onChange }: {
  label: string; hint?: string; value: string | number; type?: string; placeholder?: string;
  onChange: (value: string) => void;
}) {
  return <label className="wizard-field"><span>{label}</span><input type={type} value={value} placeholder={placeholder} onChange={event => onChange(event.target.value)} />{hint && <small>{hint}</small>}</label>;
}

function SelectField({ label, value, onChange, children, hint }: {
  label: string; value: string; onChange: (value: string) => void; children: React.ReactNode; hint?: string;
}) {
  return <label className="wizard-field"><span>{label}</span><select value={value} onChange={event => onChange(event.target.value)}>{children}</select>{hint && <small>{hint}</small>}</label>;
}

export function ConfigWizard({ source, onChange, notify }: WizardProps) {
  const parsed = useMemo(() => {
    try { return { config: object(yaml.load(source)), error: "" }; }
    catch (error) { return { config: {}, error: (error as Error).message }; }
  }, [source]);
  const jobs = object(parsed.config.jobs);
  const destinations = object(parsed.config.destinations);
  const jobNames = Object.keys(jobs);
  const destinationNames = Object.keys(destinations);
  const [selectedJob, setSelectedJob] = useState("");
  const [step, setStep] = useState(0);
  const [nameDraft, setNameDraft] = useState("");
  const [newDestinationName, setNewDestinationName] = useState("");
  const [newNotifierName, setNewNotifierName] = useState("");
  const [newNotifierType, setNewNotifierType] = useState("webhook");

  useEffect(() => {
    if (!selectedJob || !jobs[selectedJob]) setSelectedJob(jobNames[0] ?? "");
  }, [jobNames.join("\0"), selectedJob]);
  useEffect(() => { setNameDraft(selectedJob); }, [selectedJob]);

  if (parsed.error) return <div className="wizard-invalid"><h3>Il wizard non può aprire questo YAML</h3><p>{parsed.error}</p><p>Correggilo dalla modalità YAML, poi torna al wizard.</p></div>;

  function mutate(change: (config: ObjectMap) => void) {
    const copy = structuredClone(parsed.config) as ObjectMap;
    if (!copy.jobs || typeof copy.jobs !== "object") copy.jobs = {};
    if (!copy.destinations || typeof copy.destinations !== "object") copy.destinations = {};
    change(copy);
    onChange(dump(copy));
  }

  function addJob() {
    const jobName = uniqueName("nuovo-backup", jobNames);
    mutate(config => {
      const nextJobs = object(config.jobs);
      const nextDestinations = object(config.destinations);
      let destination = Object.keys(nextDestinations).find(name => text(object(nextDestinations[name]).type) === "restic");
      if (!destination) {
        destination = uniqueName("repository-restic", Object.keys(nextDestinations));
        nextDestinations[destination] = destinationDefaults("restic");
      }
      nextJobs[jobName] = {
        source: sourceDefaults("folder"), destination, engine: "restic", compression: "zstd",
        schedule: "0 3 * * *", verify_restore: "weekly", max_backup_age_hours: 48,
        retention: { daily: 7, weekly: 4, monthly: 6 },
        notify: { on_success: [], on_failure: [], on_verify: [] }, pre: [], post: [],
      };
    });
    setSelectedJob(jobName);
    setStep(0);
    notify({ kind: "info", text: "Nuovo job aggiunto. Completa i cinque passi e poi salva e applica." });
  }

  function duplicateJob() {
    if (!selectedJob) return;
    const copyName = uniqueName(`${selectedJob}-copia`, jobNames);
    mutate(config => { object(config.jobs)[copyName] = structuredClone(object(config.jobs)[selectedJob]); });
    setSelectedJob(copyName);
    setStep(0);
  }

  function removeJob() {
    if (!selectedJob || !window.confirm(`Eliminare il job “${selectedJob}”? La destinazione condivisa non verrà eliminata.`)) return;
    const remaining = jobNames.filter(name => name !== selectedJob);
    mutate(config => { delete object(config.jobs)[selectedJob]; });
    setSelectedJob(remaining[0] ?? "");
    setStep(0);
  }

  function renameJob() {
    const nextName = nameDraft.trim();
    if (!selectedJob || nextName === selectedJob) return;
    if (!/^[a-zA-Z0-9_-]+$/.test(nextName)) {
      setNameDraft(selectedJob);
      notify({ kind: "error", text: "Il nome del job può contenere solo lettere, numeri, trattini e underscore." });
      return;
    }
    if (jobs[nextName]) {
      setNameDraft(selectedJob);
      notify({ kind: "error", text: `Esiste già un job chiamato ${nextName}.` });
      return;
    }
    mutate(config => {
      const nextJobs = object(config.jobs);
      nextJobs[nextName] = nextJobs[selectedJob];
      delete nextJobs[selectedJob];
    });
    setSelectedJob(nextName);
  }

  function patchJob(patch: ObjectMap) {
    if (!selectedJob) return;
    mutate(config => Object.assign(object(object(config.jobs)[selectedJob]), patch));
  }

  function patchSource(patch: ObjectMap) {
    const job = object(jobs[selectedJob]);
    patchJob({ source: { ...object(job.source), ...patch } });
  }

  function patchDestination(patch: ObjectMap) {
    const destinationName = text(object(jobs[selectedJob]).destination);
    if (!destinationName) return;
    mutate(config => Object.assign(object(object(config.destinations)[destinationName]), patch));
  }

  function addDestination() {
    const requested = newDestinationName.trim();
    if (!/^[a-zA-Z0-9_-]+$/.test(requested)) {
      notify({ kind: "error", text: "Inserisci un nome destinazione usando lettere, numeri, trattini o underscore." });
      return;
    }
    if (destinations[requested]) {
      notify({ kind: "error", text: `La destinazione ${requested} esiste già.` });
      return;
    }
    mutate(config => {
      object(config.destinations)[requested] = destinationDefaults("fs");
      object(object(config.jobs)[selectedJob]).destination = requested;
    });
    setNewDestinationName("");
  }

  function addNotifier() {
    const requested = newNotifierName.trim();
    if (!/^[a-zA-Z0-9_-]+$/.test(requested)) {
      notify({ kind: "error", text: "Inserisci un nome notifier usando lettere, numeri, trattini o underscore." });
      return;
    }
    if (object(parsed.config.notifiers)[requested]) {
      notify({ kind: "error", text: `Il notifier ${requested} esiste già.` });
      return;
    }
    mutate(config => {
      if (!config.notifiers || typeof config.notifiers !== "object") config.notifiers = {};
      object(config.notifiers)[requested] = notifierDefaults(newNotifierType);
    });
    setNewNotifierName("");
  }

  function patchNotifier(name: string, patch: ObjectMap) {
    mutate(config => {
      if (!config.notifiers || typeof config.notifiers !== "object") config.notifiers = {};
      Object.assign(object(object(config.notifiers)[name]), patch);
    });
  }

  function removeNotifier(name: string) {
    if (!window.confirm(`Eliminare il notifier “${name}”? Verrà rimosso anche dalle regole di notifica dei job.`)) return;
    mutate(config => {
      const notifiersMap = object(config.notifiers);
      delete notifiersMap[name];
      const jobsMap = object(config.jobs);
      for (const jobName of Object.keys(jobsMap)) {
        const notifyConfigForJob = object(object(jobsMap[jobName]).notify);
        for (const event of ["on_success", "on_failure", "on_verify"] as const) {
          if (Array.isArray(notifyConfigForJob[event])) {
            notifyConfigForJob[event] = (notifyConfigForJob[event] as unknown[]).filter(item => item !== name);
          }
        }
      }
    });
  }

  if (!selectedJob || !jobs[selectedJob]) return <div className="wizard-empty"><div className="empty-icon">＋</div><h3>Crea il primo job di backup</h3><p>Un’installazione può contenere tutti i job che servono, ciascuno con sorgente, destinazione e pianificazione indipendenti.</p><button onClick={addJob}>Crea il primo job</button></div>;

  const job = object(jobs[selectedJob]);
  const sourceConfig = object(job.source);
  const sourceType = text(sourceConfig.type, "folder");
  const destinationName = text(job.destination);
  const destination = object(destinations[destinationName]);
  const destinationType = text(destination.type, "fs");
  const destinationEnvironment = object(destination.environment);
  const maintenanceEnvironment = object(destination.maintenance_environment);
  const objectLock = object(destination.object_lock);
  const retention = object(job.retention);
  const notifyConfig = object(job.notify);
  const notifiers = object(parsed.config.notifiers);
  const notifierNames = Object.keys(notifiers);
  const manageableNotifierNames = notifierNames.filter(name => text(object(notifiers[name]).type) !== "hub");
  const usersOfDestination = jobNames.filter(name => text(object(jobs[name]).destination) === destinationName);

  function sourceStep() {
    return <div className="wizard-form"><div className="form-intro"><h3>Cosa vuoi salvare?</h3><p>Scegli il tipo di dato e indica dove Backuppo può trovarlo.</p></div>
      <SelectField label="Tipo di sorgente" value={sourceType} onChange={value => patchJob({ source: sourceDefaults(value) })}>
        <option value="folder">Cartella</option><option value="sqlite">Database SQLite</option><option value="postgres">Database PostgreSQL</option><option value="mysql">Database MySQL</option><option value="docker_volume">Volume Docker</option><option value="command">Output di un comando</option><option value="disk_image">Disco o device</option><option value="libvirt_vm">Macchina virtuale libvirt</option><option value="proxmox_vm">VM/container Proxmox</option>
      </SelectField>
      {sourceType === "folder" && <><Field label="Cartella da salvare" value={text(sourceConfig.path)} placeholder="/srv/data" onChange={value => patchSource({ path: value })} /><Field label="Esclusioni" hint="Percorsi o pattern separati da virgola." value={list(sourceConfig.exclude).join(", ")} onChange={value => patchSource({ exclude: value.split(",").map(item => item.trim()).filter(Boolean) })} /></>}
      {sourceType === "sqlite" && <Field label="File SQLite" value={text(sourceConfig.path)} placeholder="/srv/data/app.sqlite" onChange={value => patchSource({ path: value })} />}
      {(sourceType === "postgres" || sourceType === "mysql") && <div className="field-grid"><Field label="Host" value={text(sourceConfig.host)} onChange={value => patchSource({ host: value })} /><Field label="Porta" type="number" value={number(sourceConfig.port, sourceType === "postgres" ? 5432 : 3306)} onChange={value => patchSource({ port: Number(value) })} /><Field label="Utente" value={text(sourceConfig.user)} onChange={value => patchSource({ user: value })} /><Field label="Database" value={text(sourceConfig.database)} onChange={value => patchSource({ database: value })} /><Field label="Variabile password" hint="Nome della variabile d’ambiente, non la password." value={text(sourceConfig.password_env)} onChange={value => patchSource({ password_env: value })} /><Field label="Container (opzionale)" hint="Esegue il dump con docker exec." value={text(sourceConfig.container)} onChange={value => patchSource({ container: value || undefined })} /></div>}
      {sourceType === "docker_volume" && <Field label="Nome del volume" value={text(sourceConfig.volume)} onChange={value => patchSource({ volume: value })} />}
      {sourceType === "command" && <><Field label="Comando" value={text(sourceConfig.command)} onChange={value => patchSource({ command: value })} /><Field label="Argomenti" hint="Separati da virgola; non vengono interpretati da una shell." value={list(sourceConfig.args).join(", ")} onChange={value => patchSource({ args: value.split(",").map(item => item.trim()).filter(Boolean) })} /><Field label="Nome file prodotto" value={text(sourceConfig.output_filename, "output")} onChange={value => patchSource({ output_filename: value })} /></>}
      {sourceType === "disk_image" && <div className="field-grid"><Field label="File o device" value={text(sourceConfig.path)} onChange={value => patchSource({ path: value })} /><Field label="Nome immagine" value={text(sourceConfig.output_filename, "disk.img")} onChange={value => patchSource({ output_filename: value })} /></div>}
      {sourceType === "libvirt_vm" && <div className="field-grid"><Field label="Nome VM" value={text(sourceConfig.name)} onChange={value => patchSource({ name: value })} /><Field label="Binario virsh" value={text(sourceConfig.virsh_binary, "virsh")} onChange={value => patchSource({ virsh_binary: value })} /></div>}
      {sourceType === "proxmox_vm" && <div className="field-grid"><Field label="VMID" type="number" value={number(sourceConfig.vmid, 100)} onChange={value => patchSource({ vmid: Number(value) })} /><SelectField label="Modalità" value={text(sourceConfig.mode, "snapshot")} onChange={value => patchSource({ mode: value })}><option value="snapshot">Snapshot (non ferma la VM)</option><option value="suspend">Suspend (sospende brevemente)</option><option value="stop">Stop (spegne per la durata del backup)</option></SelectField><Field label="Binario vzdump" value={text(sourceConfig.vzdump_binary, "vzdump")} onChange={value => patchSource({ vzdump_binary: value })} /></div>}
    </div>;
  }

  function destinationStep() {
    const retryConfig = object(destination.retry);
    function advancedTransferFields(extra?: React.ReactNode) {
      return <details className="advanced-fields">
        <summary>Opzioni avanzate</summary>
        <div className="field-grid">
          <Field label="Tentativi massimi" type="number" value={number(retryConfig.max_times, 3)} onChange={value => patchDestination({ retry: { ...retryConfig, max_times: Number(value) } })} />
          <Field label="Limite banda (KiB/s, opzionale)" type="number" value={optionalNumber(destination.bandwidth_limit_kib_s)} onChange={value => patchDestination({ bandwidth_limit_kib_s: value ? Number(value) : undefined })} />
        </div>
        {extra}
      </details>;
    }
    return <div className="wizard-form"><div className="form-intro"><h3>Dove vuoi conservare il backup?</h3><p>Puoi riutilizzare la stessa destinazione per più job oppure crearne una nuova.</p></div>
      <SelectField label="Destinazione usata da questo job" value={destinationName} onChange={value => patchJob({ destination: value, engine: text(object(destinations[value]).type) === "restic" ? "restic" : "archive" })}>
        {destinationNames.map(name => <option key={name} value={name}>{name}</option>)}
      </SelectField>
      <div className="inline-create"><input placeholder="Nome nuova destinazione" value={newDestinationName} onChange={event => setNewDestinationName(event.target.value)} /><button className="secondary" onClick={addDestination}>Aggiungi destinazione</button></div>
      {usersOfDestination.length > 1 && <p className="shared-warning">Questa destinazione è condivisa da {usersOfDestination.length} job: {usersOfDestination.join(", ")}. Le modifiche valgono per tutti.</p>}
      <SelectField label="Tipo di destinazione" value={destinationType} onChange={value => { mutate(config => { object(config.destinations)[destinationName] = destinationDefaults(value); object(object(config.jobs)[selectedJob]).engine = value === "restic" ? "restic" : "archive"; }); }}>
        <option value="fs">Disco o cartella locale</option><option value="sftp">Server SFTP</option><option value="s3">S3 / MinIO</option><option value="webdav">WebDAV</option><option value="google_drive">Google Drive</option><option value="dropbox">Dropbox</option><option value="one_drive">OneDrive</option><option value="restic">Repository Restic</option>
      </SelectField>
      {destinationType === "fs" && <Field label="Cartella di destinazione" value={text(destination.root)} onChange={value => patchDestination({ root: value })} />}
      {destinationType === "sftp" && <><div className="field-grid"><Field label="Host" value={text(destination.host)} onChange={value => patchDestination({ host: value })} /><Field label="Porta" type="number" value={number(destination.port, 22)} onChange={value => patchDestination({ port: Number(value) })} /><Field label="Utente" value={text(destination.user)} onChange={value => patchDestination({ user: value })} /><Field label="Cartella remota" value={text(destination.root)} onChange={value => patchDestination({ root: value })} /></div><div className="field-grid"><Field label="File chiave privata" value={text(destination.key_path)} onChange={value => patchDestination({ key_path: value || undefined })} /><Field label="Variabile password" value={text(destination.password_env)} onChange={value => patchDestination({ password_env: value || undefined })} /><Field label="Fingerprint host" value={text(destination.host_key_fingerprint)} onChange={value => patchDestination({ host_key_fingerprint: value || undefined })} /></div>{advancedTransferFields()}</>}
      {destinationType === "s3" && <><div className="field-grid"><Field label="Bucket" value={text(destination.bucket)} onChange={value => patchDestination({ bucket: value })} /><Field label="Regione" value={text(destination.region)} onChange={value => patchDestination({ region: value || undefined })} /><Field label="Endpoint personalizzato" value={text(destination.endpoint)} onChange={value => patchDestination({ endpoint: value || undefined })} /><Field label="Prefisso nel bucket" value={text(destination.root)} onChange={value => patchDestination({ root: value || undefined })} /></div><div className="field-grid"><Field label="Variabile access key" value={text(destination.access_key_id_env)} onChange={value => patchDestination({ access_key_id_env: value })} /><Field label="Variabile secret key" value={text(destination.secret_access_key_env)} onChange={value => patchDestination({ secret_access_key_env: value })} /></div>{advancedTransferFields(<label className="check-row"><input type="checkbox" checked={destination.virtual_host_style === true} onChange={event => patchDestination({ virtual_host_style: event.target.checked })} /> Stile virtual-hosted (<code>bucket.endpoint</code>) invece di path-style</label>)}</>}
      {destinationType === "webdav" && <><div className="field-grid"><Field label="URL" value={text(destination.url)} onChange={value => patchDestination({ url: value })} /><Field label="Utente" value={text(destination.user)} onChange={value => patchDestination({ user: value || undefined })} /><Field label="Variabile password" value={text(destination.password_env)} onChange={value => patchDestination({ password_env: value || undefined })} /></div>{advancedTransferFields()}</>}
      {["google_drive", "dropbox", "one_drive"].includes(destinationType) && <div className="field-grid"><Field label="Cartella remota" value={text(destination.root)} onChange={value => patchDestination({ root: value || undefined })} /><Field label="Variabile access token" value={text(destination.access_token_env)} onChange={value => patchDestination({ access_token_env: value })} /><Field label="Variabile refresh token" value={text(destination.refresh_token_env)} onChange={value => patchDestination({ refresh_token_env: value || undefined })} /><Field label="Client ID (opzionale)" value={text(destination.client_id)} onChange={value => patchDestination({ client_id: value || undefined })} /><Field label="Variabile client secret" value={text(destination.client_secret_env)} onChange={value => patchDestination({ client_secret_env: value || undefined })} /></div>}
      {destinationType === "restic" && <>
        <div className="field-grid"><Field label="Repository" value={text(destination.repository)} onChange={value => patchDestination({ repository: value })} /><Field label="Variabile password" value={text(destination.password_env)} onChange={value => patchDestination({ password_env: value })} /><Field label="Binario Restic" value={text(destination.binary, "restic")} onChange={value => patchDestination({ binary: value })} /></div>
        <label className="check-row"><input type="checkbox" checked={destination.initialize !== false} onChange={event => patchDestination({ initialize: event.target.checked })} /> Inizializza il repository alla prima scrittura se non esiste già</label>
        <label className="check-row"><input type="checkbox" checked={destination.append_only === true} onChange={event => patchDestination({ append_only: event.target.checked, ...(!event.target.checked ? { object_lock: undefined } : {}) })} /> Repository append-only: il daemon non può eseguire retention o prune</label>
        {destination.append_only === true && <>
          <div className="form-intro subsection"><h3>Credenziali S3 separate</h3><p>L’identità di backup può scrivere; quella amministrativa viene caricata solo da <code>bkpo maintain</code>.</p></div>
          <div className="field-grid">
            <Field label="Access key backup (variabile)" value={text(destinationEnvironment.AWS_ACCESS_KEY_ID)} onChange={value => patchDestination({ environment: { ...destinationEnvironment, AWS_ACCESS_KEY_ID: value } })} />
            <Field label="Secret key backup (variabile)" value={text(destinationEnvironment.AWS_SECRET_ACCESS_KEY)} onChange={value => patchDestination({ environment: { ...destinationEnvironment, AWS_SECRET_ACCESS_KEY: value } })} />
            <Field label="Access key amministrativa (variabile)" value={text(maintenanceEnvironment.AWS_ACCESS_KEY_ID)} onChange={value => patchDestination({ maintenance_environment: { ...maintenanceEnvironment, AWS_ACCESS_KEY_ID: value } })} />
            <Field label="Secret key amministrativa (variabile)" value={text(maintenanceEnvironment.AWS_SECRET_ACCESS_KEY)} onChange={value => patchDestination({ maintenance_environment: { ...maintenanceEnvironment, AWS_SECRET_ACCESS_KEY: value } })} />
          </div>
          <label className="check-row"><input type="checkbox" checked={destination.object_lock != null} onChange={event => patchDestination({ object_lock: event.target.checked ? { bucket: "backups", region: "eu-central-1", access_key_id_env: "S3_LOCK_CHECK_ACCESS_KEY", secret_access_key_env: "S3_LOCK_CHECK_SECRET_KEY", expected_mode: "compliance", minimum_retention_days: 30 } : undefined })} /> Verifica S3 Object Lock prima della messa in produzione</label>
          {destination.object_lock != null && <div className="field-grid">
            <Field label="Bucket Object Lock" value={text(objectLock.bucket)} onChange={value => patchDestination({ object_lock: { ...objectLock, bucket: value } })} />
            <Field label="Regione" value={text(objectLock.region)} onChange={value => patchDestination({ object_lock: { ...objectLock, region: value } })} />
            <Field label="Endpoint S3 (opzionale)" value={text(objectLock.endpoint)} onChange={value => patchDestination({ object_lock: { ...objectLock, endpoint: value || undefined } })} />
            <SelectField label="Modalità minima" value={text(objectLock.expected_mode, "compliance")} onChange={value => patchDestination({ object_lock: { ...objectLock, expected_mode: value } })}><option value="compliance">Compliance</option><option value="governance">Governance</option></SelectField>
            <Field label="Retention minima (giorni)" type="number" value={number(objectLock.minimum_retention_days, 30)} onChange={value => patchDestination({ object_lock: { ...objectLock, minimum_retention_days: Number(value) } })} />
            <Field label="Access key verifica (variabile)" value={text(objectLock.access_key_id_env)} onChange={value => patchDestination({ object_lock: { ...objectLock, access_key_id_env: value } })} />
            <Field label="Secret key verifica (variabile)" value={text(objectLock.secret_access_key_env)} onChange={value => patchDestination({ object_lock: { ...objectLock, secret_access_key_env: value } })} />
          </div>}
        </>}
      </>}
    </div>;
  }

  function scheduleStep() {
    const schedule = text(job.schedule, "0 3 * * *");
    const preset = SCHEDULES[schedule] ? schedule : "custom";
    return <div className="wizard-form"><div className="form-intro"><h3>Quando deve partire?</h3><p>Scegli una frequenza comune oppure inserisci un’espressione cron a cinque campi.</p></div>
      <SelectField label="Frequenza" value={preset} onChange={value => value !== "custom" && patchJob({ schedule: value })}>{Object.entries(SCHEDULES).map(([value, label]) => <option key={value} value={value}>{label}</option>)}<option value="custom">Personalizzata (cron)</option></SelectField>
      {preset === "custom" && <Field label="Espressione cron" value={schedule} hint="Minuto, ora, giorno del mese, mese, giorno della settimana." onChange={value => patchJob({ schedule: value })} />}
      <div className="form-intro subsection"><h3>Per quanto tempo conservarli?</h3><p>Lascia vuoto un campo per non mantenere quella classe di copie.</p></div>
      <div className="field-grid three"><Field label="Backup giornalieri" type="number" value={optionalNumber(retention.daily)} onChange={value => patchJob({ retention: { ...retention, daily: value ? Number(value) : undefined } })} /><Field label="Backup settimanali" type="number" value={optionalNumber(retention.weekly)} onChange={value => patchJob({ retention: { ...retention, weekly: value ? Number(value) : undefined } })} /><Field label="Backup mensili" type="number" value={optionalNumber(retention.monthly)} onChange={value => patchJob({ retention: { ...retention, monthly: value ? Number(value) : undefined } })} /></div>
    </div>;
  }

  function protectionStep() {
    const encryption = object(job.encryption);
    const encryptionType = text(encryption.type, "none");
    return <div className="wizard-form"><div className="form-intro"><h3>Protezione e controlli</h3><p>Configura compressione, cifratura e verifica periodica del ripristino.</p></div>
      <div className="field-grid"><SelectField label="Compressione" value={text(job.compression, "zstd")} onChange={value => patchJob({ compression: value })}><option value="zstd">Zstandard (consigliata)</option><option value="none">Nessuna</option></SelectField><SelectField label="Verifica ripristino" value={text(job.verify_restore, "weekly")} onChange={value => patchJob({ verify_restore: value })}><option value="never">Mai</option><option value="every">Dopo ogni backup</option><option value="daily">Una volta al giorno</option><option value="weekly">Una volta a settimana</option></SelectField><Field label="Avvisa se il backup supera (ore)" type="number" value={number(job.max_backup_age_hours, 48)} onChange={value => patchJob({ max_backup_age_hours: value ? Number(value) : undefined })} /></div>
      {destinationType !== "restic" && <><SelectField label="Cifratura" value={encryptionType} onChange={value => patchJob({ encryption: value === "age" ? { type: "age", passphrase_env: "BACKUP_PASSPHRASE" } : { type: "none" } })}><option value="none">Nessuna</option><option value="age">Age</option></SelectField>{encryptionType === "age" && <div className="field-grid"><Field label="Variabile passphrase" value={text(encryption.passphrase_env)} onChange={value => patchJob({ encryption: { ...encryption, passphrase_env: value || undefined } })} /><Field label="Variabile chiave Age" value={text(encryption.key_env)} hint="Usa passphrase oppure chiave." onChange={value => patchJob({ encryption: { ...encryption, key_env: value || undefined } })} /></div>}</>}
      <div className="form-intro subsection"><h3>Canali di notifica</h3><p>Crea e configura qui i notifier (Telegram, email, webhook, ntfy); il collegamento all'Hub si gestisce dalla pagina “Collegamento Hub”.</p></div>
      <div className="notifier-manage-list">
        {manageableNotifierNames.length === 0 && <p className="muted">Nessun notifier configurato.</p>}
        {manageableNotifierNames.map(name => {
          const notifierConfig = object(notifiers[name]);
          const notifierType = text(notifierConfig.type, "webhook");
          return <div key={name} className="notifier-manage-item">
            <div className="notifier-manage-head"><strong>{name}</strong><span className="muted">{notifierType}</span><button className="danger" onClick={() => removeNotifier(name)}>Elimina</button></div>
            {notifierType === "telegram" && <div className="field-grid"><Field label="Variabile token bot" value={text(notifierConfig.token_env)} onChange={value => patchNotifier(name, { token_env: value })} /><Field label="Chat ID" value={text(notifierConfig.chat_id)} onChange={value => patchNotifier(name, { chat_id: value })} /></div>}
            {notifierType === "smtp" && <div className="field-grid"><Field label="Host" value={text(notifierConfig.host)} onChange={value => patchNotifier(name, { host: value })} /><Field label="Porta" type="number" value={number(notifierConfig.port, 587)} onChange={value => patchNotifier(name, { port: Number(value) })} /><Field label="Utente" value={text(notifierConfig.user)} onChange={value => patchNotifier(name, { user: value })} /><Field label="Variabile password" value={text(notifierConfig.password_env)} onChange={value => patchNotifier(name, { password_env: value })} /><Field label="Mittente" value={text(notifierConfig.from)} onChange={value => patchNotifier(name, { from: value })} /><Field label="Destinatari" hint="Separati da virgola." value={list(notifierConfig.to).join(", ")} onChange={value => patchNotifier(name, { to: value.split(",").map(item => item.trim()).filter(Boolean) })} /></div>}
            {notifierType === "ntfy" && <div className="field-grid"><Field label="URL server" value={text(notifierConfig.url, "https://ntfy.sh")} onChange={value => patchNotifier(name, { url: value })} /><Field label="Topic" value={text(notifierConfig.topic)} onChange={value => patchNotifier(name, { topic: value })} /><Field label="Variabile token (opzionale)" value={text(notifierConfig.token_env)} onChange={value => patchNotifier(name, { token_env: value || undefined })} /></div>}
            {notifierType === "webhook" && <div className="field-grid"><Field label="URL webhook" value={text(notifierConfig.url)} onChange={value => patchNotifier(name, { url: value })} /></div>}
          </div>;
        })}
      </div>
      <div className="inline-create">
        <input placeholder="Nome nuovo notifier" value={newNotifierName} onChange={event => setNewNotifierName(event.target.value)} />
        <select value={newNotifierType} onChange={event => setNewNotifierType(event.target.value)}>
          <option value="webhook">Webhook</option><option value="telegram">Telegram</option><option value="smtp">Email (SMTP)</option><option value="ntfy">ntfy</option>
        </select>
        <button className="secondary" onClick={addNotifier}>Aggiungi notifier</button>
      </div>
      <div className="form-intro subsection"><h3>Regole per questo job</h3><p>Scegli quali notifier già creati usare e per quali eventi.</p></div>
      {notifierNames.length === 0 ? <p className="muted">Nessun notifier configurato: il job funzionerà senza notifiche.</p> : <div className="notifier-grid">{notifierNames.map(name => <label key={name}><strong>{name}</strong>{(["on_failure", "on_success", "on_verify"] as const).map(event => { const active = list(notifyConfig[event]).includes(name); return <span key={event}><input type="checkbox" checked={active} onChange={() => { const values = list(notifyConfig[event]); patchJob({ notify: { ...notifyConfig, [event]: active ? values.filter(item => item !== name) : [...values, name] } }); }} /> {event === "on_failure" ? "Errori" : event === "on_success" ? "Successi" : "Verifiche"}</span>; })}</label>)}</div>}
    </div>;
  }

  function reviewStep() {
    return <div className="wizard-form"><div className="form-intro"><h3>Controlla il job</h3><p>Se il riepilogo è corretto, usa “Salva e applica” in alto. Potrai aggiungere altri job subito dopo.</p></div><dl className="review-list"><div><dt>Job</dt><dd>{selectedJob}</dd></div><div><dt>Sorgente</dt><dd>{sourceType} · {text(sourceConfig.path) || text(sourceConfig.database) || text(sourceConfig.volume) || text(sourceConfig.command) || text(sourceConfig.name)}</dd></div><div><dt>Destinazione</dt><dd>{destinationName} · {destinationType}</dd></div><div><dt>Pianificazione</dt><dd>{SCHEDULES[text(job.schedule)] ?? text(job.schedule)}</dd></div><div><dt>Conservazione</dt><dd>{number(retention.daily, 0)} giornalieri · {number(retention.weekly, 0)} settimanali · {number(retention.monthly, 0)} mensili</dd></div><div><dt>Verifica</dt><dd>{text(job.verify_restore, "weekly")}</dd></div></dl><div className="review-ready"><span>✓</span><div><strong>Job pronto per la validazione</strong><p>Il server controllerà percorsi, riferimenti, credenziali e sintassi cron prima del salvataggio.</p></div></div></div>;
  }

  const contents = [sourceStep, destinationStep, scheduleStep, protectionStep, reviewStep];
  return <div className="config-wizard"><aside className="job-sidebar"><div className="job-sidebar-head"><div><span className="eyebrow">Questa installazione</span><h3>{jobNames.length} job</h3></div><button title="Aggiungi un job" onClick={addJob}>＋ Nuovo</button></div><div className="job-list">{jobNames.map(name => { const item = object(jobs[name]); return <button key={name} className={name === selectedJob ? "selected" : ""} onClick={() => { setSelectedJob(name); setStep(0); }}><strong>{name}</strong><small>{text(object(item.source).type)} → {text(item.destination)}</small></button>; })}</div></aside>
    <div className="wizard-main"><div className="job-toolbar"><label><span>Nome job</span><input value={nameDraft} onChange={event => setNameDraft(event.target.value)} onBlur={renameJob} onKeyDown={event => event.key === "Enter" && event.currentTarget.blur()} /></label><div className="actions"><button className="secondary" onClick={duplicateJob}>Duplica</button><button className="danger" onClick={removeJob}>Elimina</button></div></div>
      <ol className="wizard-steps">{STEPS.map((label, index) => <li key={label} className={index === step ? "active" : index < step ? "done" : ""}><button onClick={() => setStep(index)}><span>{index < step ? "✓" : index + 1}</span>{label}</button></li>)}</ol>
      <div className="wizard-content">{contents[step]()}</div>
      <div className="wizard-nav"><button className="secondary" disabled={step === 0} onClick={() => setStep(value => Math.max(0, value - 1))}>Indietro</button><span>Passo {step + 1} di {STEPS.length}</span><button disabled={step === STEPS.length - 1} onClick={() => setStep(value => Math.min(STEPS.length - 1, value + 1))}>Continua</button></div>
    </div>
  </div>;
}
