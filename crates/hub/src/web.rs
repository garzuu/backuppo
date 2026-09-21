pub const INDEX_HTML: &str = r#"<!doctype html>
<html lang="it"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Backuppo Hub</title><style>
body{font:15px system-ui;margin:2rem auto;max-width:1100px;padding:0 1rem;background:#111827;color:#e5e7eb}
table{width:100%;border-collapse:collapse;background:#1f2937;margin-bottom:1.5rem}
th,td{padding:.6rem;border-bottom:1px solid #374151;text-align:left}
input{padding:.4rem;margin-right:.4rem;background:#1f2937;color:#e5e7eb;border:1px solid #374151}
button{padding:.45rem .7rem;cursor:pointer}
.online{color:#34d399}.offline{color:#f87171}.unknown{color:#9ca3af}
.success{color:#34d399}.failure{color:#f87171}.report{color:#9ca3af}.restore_verified{color:#60a5fa}
#login{max-width:320px}
</style></head><body>
<h1>Backuppo Hub</h1>
<div id="login">
  <h2>Login</h2>
  <input id="username" placeholder="utente">
  <input id="password" type="password" placeholder="password">
  <button onclick="login()">Entra</button>
  <p id="login-message"></p>
</div>
<div id="app" style="display:none">
  <p><button onclick="logout()">Esci</button></p>
  <h2>Siti</h2>
  <table><thead><tr><th>Sito</th><th>Cliente</th><th>Stato</th><th>Ultimo heartbeat</th><th>Ultimo evento</th></tr></thead>
  <tbody id="sites"></tbody></table>

  <h2>Nuovo cliente / sito</h2>
  <input id="customer-name" placeholder="nome cliente">
  <button onclick="createCustomer()">Crea cliente</button>
  <br><br>
  <input id="site-customer-id" placeholder="id cliente">
  <input id="site-name" placeholder="nome sito">
  <button onclick="createSite()">Crea sito (genera token agent)</button>
  <p id="new-token"></p>

  <h2>Esecuzioni per sito</h2>
  <input id="executions-site-id" placeholder="id sito">
  <button onclick="loadExecutions()">Carica</button>
  <table><thead><tr><th>ID</th><th>Job</th><th>Tipo</th><th>Dettaglio</th><th>Byte</th><th>Ricevuto</th></tr></thead>
  <tbody id="executions"></tbody></table>
</div>
<script>
const esc=s=>String(s??'—').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fmt=t=>t?new Date(t*1000).toLocaleString():'—';

function tokens(){
  try { return JSON.parse(localStorage.getItem('backuppo-hub-tokens')||'null'); } catch { return null; }
}
function saveTokens(t){ localStorage.setItem('backuppo-hub-tokens', JSON.stringify(t)); }
function clearTokens(){ localStorage.removeItem('backuppo-hub-tokens'); }

async function authedFetch(path, options={}){
  const t = tokens();
  if (!t) { showLogin(); throw new Error('non autenticato'); }
  options.headers = Object.assign({}, options.headers, {'Authorization': 'Bearer ' + t.access_token});
  let response = await fetch(path, options);
  if (response.status === 401 && t.refresh_token) {
    const refreshed = await fetch('/v1/auth/refresh', {
      method: 'POST', headers: {'content-type': 'application/json'},
      body: JSON.stringify({refresh_token: t.refresh_token})
    });
    if (refreshed.ok) {
      saveTokens(await refreshed.json());
      options.headers['Authorization'] = 'Bearer ' + tokens().access_token;
      response = await fetch(path, options);
    } else {
      clearTokens();
      showLogin();
      throw new Error('sessione scaduta');
    }
  }
  return response;
}

function showLogin(){ login_el.style.display='block'; app.style.display='none'; }
function showApp(){ login_el.style.display='none'; app.style.display='block'; loadSites(); }

async function login(){
  const response = await fetch('/v1/auth/login', {
    method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({username: username.value, password: password.value})
  });
  if (!response.ok) { document.getElementById('login-message').textContent = 'credenziali non valide'; return; }
  saveTokens(await response.json());
  showApp();
}
function logout(){ clearTokens(); showLogin(); }

async function loadSites(){
  const response = await authedFetch('/v1/sites');
  const rows = await response.json();
  sites.innerHTML = rows.map(s => `<tr><td>${esc(s.name)} (#${s.id})</td><td>${s.customer_id}</td>` +
    `<td class="${esc(s.status)}">${esc(s.status)}</td><td>${fmt(s.last_heartbeat_at)}</td><td>${fmt(s.last_event_at)}</td></tr>`).join('');
}

async function createCustomer(){
  await authedFetch('/v1/customers', {
    method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({name: document.getElementById('customer-name').value})
  });
  loadSites();
}

async function createSite(){
  const response = await authedFetch('/v1/sites', {
    method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({
      customer_id: Number(document.getElementById('site-customer-id').value),
      name: document.getElementById('site-name').value
    })
  });
  const body = await response.json();
  document.getElementById('new-token').textContent =
    'Token agent (mostrato una sola volta): ' + body.agent_token;
  loadSites();
}

async function loadExecutions(){
  const siteId = document.getElementById('executions-site-id').value;
  const response = await authedFetch('/v1/sites/' + siteId + '/executions');
  const rows = await response.json();
  executions.innerHTML = rows.map(e => `<tr><td>${e.id}</td><td>${esc(e.job)}</td>` +
    `<td class="${esc(e.kind)}">${esc(e.kind)}</td><td>${esc(e.detail)}</td><td>${esc(e.bytes)}</td><td>${fmt(e.received_at)}</td></tr>`).join('');
}

const login_el = document.getElementById('login');
if (tokens()) { showApp(); } else { showLogin(); }
</script></body></html>"#;
