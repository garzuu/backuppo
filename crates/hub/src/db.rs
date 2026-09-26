use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct Db {
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    /// Sola lettura + può chiedere agli agent "esegui ora"/"verifica ora",
    /// ma non gestire clienti, siti e token.
    Operator,
    ReadOnly,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
            Role::ReadOnly => "read_only",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Role::Admin),
            "operator" => Some(Role::Operator),
            "read_only" => Some(Role::ReadOnly),
            _ => None,
        }
    }
}

/// Un comando non ritirato dall'agent entro questo tempo scade: un backup
/// richiesto da un utente non deve partire ore dopo perché l'agent era offline.
pub const PENDING_TTL_SECONDS: i64 = 10 * 60;
/// Un comando ritirato ma senza esito oltre questo tempo (agent morto a metà)
/// viene chiuso come fallito, così non blocca per sempre nuove richieste.
pub const DELIVERED_TIMEOUT_SECONDS: i64 = 24 * 60 * 60;

/// Stato: `pending` → `delivered` → `done` | `failed`; oppure `expired`.
#[derive(Debug, Clone, Serialize)]
pub struct Command {
    pub id: i64,
    pub site_id: i64,
    pub kind: String,
    pub job: String,
    pub status: String,
    pub created_by: String,
    pub created_at: i64,
    pub delivered_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Customer {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub id: i64,
    pub customer_id: i64,
    pub name: String,
    pub status: String,
    pub last_heartbeat_at: Option<i64>,
    pub last_event_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentToken {
    pub id: i64,
    pub site_id: i64,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub id: i64,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Execution {
    pub id: i64,
    pub site_id: i64,
    pub job: Option<String>,
    pub kind: String,
    pub detail: Option<String>,
    pub bytes: Option<i64>,
    pub files: Option<i64>,
    pub checksum: Option<String>,
    pub received_at: i64,
}

pub struct NewExecution<'a> {
    pub site_id: i64,
    pub job: Option<&'a str>,
    pub kind: &'a str,
    pub detail: Option<&'a str>,
    pub bytes: Option<i64>,
    pub files: Option<i64>,
    pub checksum: Option<&'a str>,
    pub received_at: i64,
}

impl Db {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let db = Self { path: path.into() };
        db.initialize()?;
        Ok(db)
    }

    fn connect(&self) -> Result<Connection> {
        if self.path != Path::new(":memory:") {
            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
        let connection = Connection::open(&self.path)
            .with_context(|| format!("apertura database hub '{}'", self.path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn initialize(&self) -> Result<()> {
        self.connect()?
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS customers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE
                 );
                 CREATE TABLE IF NOT EXISTS sites (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    customer_id INTEGER NOT NULL REFERENCES customers(id),
                    name TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'unknown',
                    last_heartbeat_at INTEGER,
                    last_event_at INTEGER,
                    UNIQUE(customer_id, name)
                 );
                 CREATE TABLE IF NOT EXISTS agent_tokens (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    site_id INTEGER NOT NULL REFERENCES sites(id),
                    token_hash TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL,
                    revoked_at INTEGER
                 );
                 CREATE TABLE IF NOT EXISTS users (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    username TEXT NOT NULL UNIQUE,
                    password_hash TEXT NOT NULL,
                    role TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS refresh_tokens (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    user_id INTEGER NOT NULL REFERENCES users(id),
                    token_hash TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    revoked_at INTEGER
                 );
                 CREATE TABLE IF NOT EXISTS executions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    site_id INTEGER NOT NULL REFERENCES sites(id),
                    job TEXT,
                    kind TEXT NOT NULL,
                    detail TEXT,
                    bytes INTEGER,
                    files INTEGER,
                    checksum TEXT,
                    received_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS commands (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    site_id INTEGER NOT NULL REFERENCES sites(id),
                    kind TEXT NOT NULL,
                    job TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    created_by TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    delivered_at INTEGER,
                    finished_at INTEGER,
                    detail TEXT
                 );
                 CREATE INDEX IF NOT EXISTS commands_site_id ON commands(site_id, id DESC);
                 CREATE INDEX IF NOT EXISTS executions_site_id ON executions(site_id, id DESC);
                 CREATE INDEX IF NOT EXISTS agent_tokens_hash ON agent_tokens(token_hash);
                 CREATE INDEX IF NOT EXISTS refresh_tokens_hash ON refresh_tokens(token_hash);",
            )
            .context("inizializzazione schema hub")?;
        Ok(())
    }

    // --- customers -----------------------------------------------------

    pub fn create_customer(&self, name: &str) -> Result<Customer> {
        let connection = self.connect()?;
        connection.execute("INSERT INTO customers (name) VALUES (?1)", params![name])?;
        let id = connection.last_insert_rowid();
        Ok(Customer {
            id,
            name: name.to_string(),
        })
    }

    pub fn list_customers(&self) -> Result<Vec<Customer>> {
        let connection = self.connect()?;
        let mut statement =
            connection.prepare("SELECT id, name FROM customers ORDER BY name ASC")?;
        let rows = statement.query_map([], |row| {
            Ok(Customer {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- sites -----------------------------------------------------------

    pub fn create_site(&self, customer_id: i64, name: &str) -> Result<Site> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO sites (customer_id, name, status) VALUES (?1, ?2, 'unknown')",
            params![customer_id, name],
        )?;
        let id = connection.last_insert_rowid();
        Ok(Site {
            id,
            customer_id,
            name: name.to_string(),
            status: "unknown".to_string(),
            last_heartbeat_at: None,
            last_event_at: None,
        })
    }

    pub fn list_sites(&self) -> Result<Vec<Site>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, customer_id, name, status, last_heartbeat_at, last_event_at
             FROM sites ORDER BY name ASC",
        )?;
        let rows = statement.query_map([], site_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_site(&self, id: i64) -> Result<Option<Site>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, customer_id, name, status, last_heartbeat_at, last_event_at
                 FROM sites WHERE id = ?1",
                [id],
                site_from_row,
            )
            .optional()
            .context("lettura sito")
    }

    /// Siti online/sconosciuti il cui ultimo heartbeat è più vecchio della
    /// soglia: candidati a essere marcati offline.
    pub fn sites_overdue(&self, threshold: i64) -> Result<Vec<Site>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, customer_id, name, status, last_heartbeat_at, last_event_at
             FROM sites
             WHERE status != 'offline'
               AND last_heartbeat_at IS NOT NULL
               AND last_heartbeat_at < ?1",
        )?;
        let rows = statement.query_map([threshold], site_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn mark_site_offline(&self, id: i64) -> Result<()> {
        self.connect()?.execute(
            "UPDATE sites SET status = 'offline' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn record_heartbeat(&self, site_id: i64, at: i64) -> Result<()> {
        self.connect()?.execute(
            "UPDATE sites SET status = 'online', last_heartbeat_at = ?2 WHERE id = ?1",
            params![site_id, at],
        )?;
        Ok(())
    }

    // --- comandi -------------------------------------------------------------

    /// Chiude i comandi scaduti (vedi le costanti sopra).
    fn expire_commands(&self, connection: &Connection, now: i64) -> Result<()> {
        connection.execute(
            "UPDATE commands SET status = 'expired', finished_at = ?1,
                    detail = 'non ritirato dall''agent in tempo'
             WHERE status = 'pending' AND created_at < ?2",
            params![now, now - PENDING_TTL_SECONDS],
        )?;
        connection.execute(
            "UPDATE commands SET status = 'failed', finished_at = ?1,
                    detail = 'nessun esito ricevuto dall''agent'
             WHERE status = 'delivered' AND delivered_at < ?2",
            params![now, now - DELIVERED_TIMEOUT_SECONDS],
        )?;
        Ok(())
    }

    /// Crea un comando. `None` se per lo stesso sito/azione/job ce n'è già
    /// uno in attesa o in esecuzione (evita doppi avvii da doppio tap).
    pub fn create_command(
        &self,
        site_id: i64,
        kind: &str,
        job: &str,
        created_by: &str,
        now: i64,
    ) -> Result<Option<Command>> {
        let connection = self.connect()?;
        self.expire_commands(&connection, now)?;
        let active: i64 = connection.query_row(
            "SELECT COUNT(*) FROM commands
             WHERE site_id = ?1 AND kind = ?2 AND job = ?3
               AND status IN ('pending', 'delivered')",
            params![site_id, kind, job],
            |row| row.get(0),
        )?;
        if active > 0 {
            return Ok(None);
        }
        connection.execute(
            "INSERT INTO commands (site_id, kind, job, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![site_id, kind, job, created_by, now],
        )?;
        let id = connection.last_insert_rowid();
        Ok(Some(Command {
            id,
            site_id,
            kind: kind.to_string(),
            job: job.to_string(),
            status: "pending".to_string(),
            created_by: created_by.to_string(),
            created_at: now,
            delivered_at: None,
            finished_at: None,
            detail: None,
        }))
    }

    /// Ritira i comandi in attesa del sito e li marca `delivered`, in una
    /// transazione: due poll concorrenti non ricevono mai lo stesso comando.
    pub fn claim_commands(&self, site_id: i64, now: i64) -> Result<Vec<Command>> {
        let mut connection = self.connect()?;
        self.expire_commands(&connection, now)?;
        let tx = connection.transaction()?;
        let claimed = {
            let mut statement = tx.prepare(
                "SELECT id, site_id, kind, job, status, created_by, created_at,
                        delivered_at, finished_at, detail
                 FROM commands WHERE site_id = ?1 AND status = 'pending' ORDER BY id ASC",
            )?;
            let rows = statement.query_map([site_id], command_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for command in &claimed {
            tx.execute(
                "UPDATE commands SET status = 'delivered', delivered_at = ?2 WHERE id = ?1",
                params![command.id, now],
            )?;
        }
        tx.commit()?;
        Ok(claimed
            .into_iter()
            .map(|mut command| {
                command.status = "delivered".to_string();
                command.delivered_at = Some(now);
                command
            })
            .collect())
    }

    /// Registra l'esito. `false` se il comando non esiste, non è di questo
    /// sito o non è in stato `delivered` (es. esito già riportato).
    pub fn finish_command(
        &self,
        site_id: i64,
        id: i64,
        ok: bool,
        detail: Option<&str>,
        now: i64,
    ) -> Result<bool> {
        let changed = self.connect()?.execute(
            "UPDATE commands SET status = ?3, finished_at = ?4, detail = ?5
             WHERE id = ?1 AND site_id = ?2 AND status = 'delivered'",
            params![id, site_id, if ok { "done" } else { "failed" }, now, detail],
        )?;
        Ok(changed > 0)
    }

    pub fn list_commands(&self, site_id: i64, limit: i64, now: i64) -> Result<Vec<Command>> {
        let connection = self.connect()?;
        self.expire_commands(&connection, now)?;
        let mut statement = connection.prepare(
            "SELECT id, site_id, kind, job, status, created_by, created_at,
                    delivered_at, finished_at, detail
             FROM commands WHERE site_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![site_id, limit], command_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- agent tokens ------------------------------------------------------

    pub fn create_agent_token(&self, site_id: i64, token_hash: &str, now: i64) -> Result<i64> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO agent_tokens (site_id, token_hash, created_at) VALUES (?1, ?2, ?3)",
            params![site_id, token_hash, now],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn revoke_agent_token(&self, site_id: i64, token_id: i64, now: i64) -> Result<bool> {
        let changed = self.connect()?.execute(
            "UPDATE agent_tokens SET revoked_at = ?3
             WHERE id = ?1 AND site_id = ?2 AND revoked_at IS NULL",
            params![token_id, site_id, now],
        )?;
        Ok(changed > 0)
    }

    pub fn list_agent_tokens(&self, site_id: i64) -> Result<Vec<AgentToken>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, site_id, created_at, revoked_at
             FROM agent_tokens WHERE site_id = ?1 ORDER BY id DESC",
        )?;
        let rows = statement.query_map([site_id], |row| {
            Ok(AgentToken {
                id: row.get(0)?,
                site_id: row.get(1)?,
                created_at: row.get(2)?,
                revoked_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Sito associato a un token agent attivo (non revocato), per hash.
    pub fn site_for_agent_token(&self, token_hash: &str) -> Result<Option<Site>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT sites.id, sites.customer_id, sites.name, sites.status,
                        sites.last_heartbeat_at, sites.last_event_at
                 FROM agent_tokens
                 JOIN sites ON sites.id = agent_tokens.site_id
                 WHERE agent_tokens.token_hash = ?1 AND agent_tokens.revoked_at IS NULL",
                [token_hash],
                site_from_row,
            )
            .optional()
            .context("lettura sito per token agent")
    }

    // --- users e refresh token ------------------------------------------

    pub fn create_user(&self, username: &str, password_hash: &str, role: Role) -> Result<i64> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO users (username, password_hash, role) VALUES (?1, ?2, ?3)",
            params![username, password_hash, role.as_str()],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn list_users(&self) -> Result<Vec<UserSummary>> {
        let connection = self.connect()?;
        let mut statement =
            connection.prepare("SELECT id, username, role FROM users ORDER BY username ASC")?;
        let rows = statement.query_map([], |row| {
            Ok(UserSummary {
                id: row.get(0)?,
                username: row.get(1)?,
                role: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn username_by_id(&self, id: i64) -> Result<Option<String>> {
        self.connect()?
            .query_row("SELECT username FROM users WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .context("lettura utente")
    }

    pub fn user_by_username(&self, username: &str) -> Result<Option<(i64, String, Role)>> {
        let connection = self.connect()?;
        let row = connection
            .query_row(
                "SELECT id, password_hash, role FROM users WHERE username = ?1",
                [username],
                |row| {
                    let role: String = row.get(2)?;
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, role))
                },
            )
            .optional()?;
        Ok(row.and_then(|(id, hash, role)| Role::parse(&role).map(|role| (id, hash, role))))
    }

    pub fn store_refresh_token(
        &self,
        user_id: i64,
        token_hash: &str,
        now: i64,
        expires_at: i64,
    ) -> Result<()> {
        self.connect()?.execute(
            "INSERT INTO refresh_tokens (user_id, token_hash, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![user_id, token_hash, now, expires_at],
        )?;
        Ok(())
    }

    /// Se il refresh token è valido (non revocato, non scaduto) lo revoca
    /// (rotazione) e ritorna l'utente e il suo ruolo.
    pub fn consume_refresh_token(&self, token_hash: &str, now: i64) -> Result<Option<(i64, Role)>> {
        let connection = self.connect()?;
        let found = connection
            .query_row(
                "SELECT refresh_tokens.user_id, users.role
                 FROM refresh_tokens
                 JOIN users ON users.id = refresh_tokens.user_id
                 WHERE refresh_tokens.token_hash = ?1
                   AND refresh_tokens.revoked_at IS NULL
                   AND refresh_tokens.expires_at > ?2",
                params![token_hash, now],
                |row| {
                    let role: String = row.get(1)?;
                    Ok((row.get::<_, i64>(0)?, role))
                },
            )
            .optional()?;
        let Some((user_id, role)) = found else {
            return Ok(None);
        };
        connection.execute(
            "UPDATE refresh_tokens SET revoked_at = ?2 WHERE token_hash = ?1",
            params![token_hash, now],
        )?;
        Ok(Role::parse(&role).map(|role| (user_id, role)))
    }

    // --- executions ------------------------------------------------------

    pub fn record_execution(&self, execution: &NewExecution<'_>) -> Result<i64> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO executions
                (site_id, job, kind, detail, bytes, files, checksum, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                execution.site_id,
                execution.job,
                execution.kind,
                execution.detail,
                execution.bytes,
                execution.files,
                execution.checksum,
                execution.received_at,
            ],
        )?;
        let id = connection.last_insert_rowid();
        connection.execute(
            "UPDATE sites SET last_event_at = ?2 WHERE id = ?1",
            params![execution.site_id, execution.received_at],
        )?;
        Ok(id)
    }

    pub fn list_executions_for_site(&self, site_id: i64, limit: usize) -> Result<Vec<Execution>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, site_id, job, kind, detail, bytes, files, checksum, received_at
             FROM executions WHERE site_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = statement.query_map(params![site_id, limit], execution_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_jobs_for_site(&self, site_id: i64) -> Result<Vec<String>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT DISTINCT job FROM executions
             WHERE site_id = ?1 AND job IS NOT NULL ORDER BY job ASC",
        )?;
        let rows = statement.query_map(params![site_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn site_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Site> {
    Ok(Site {
        id: row.get(0)?,
        customer_id: row.get(1)?,
        name: row.get(2)?,
        status: row.get(3)?,
        last_heartbeat_at: row.get(4)?,
        last_event_at: row.get(5)?,
    })
}

fn execution_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Execution> {
    Ok(Execution {
        id: row.get(0)?,
        site_id: row.get(1)?,
        job: row.get(2)?,
        kind: row.get(3)?,
        detail: row.get(4)?,
        bytes: row.get(5)?,
        files: row.get(6)?,
        checksum: row.get(7)?,
        received_at: row.get(8)?,
    })
}

fn command_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Command> {
    Ok(Command {
        id: row.get(0)?,
        site_id: row.get(1)?,
        kind: row.get(2)?,
        job: row.get(3)?,
        status: row.get(4)?,
        created_by: row.get(5)?,
        created_at: row.get(6)?,
        delivered_at: row.get(7)?,
        finished_at: row.get(8)?,
        detail: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (tempfile::TempDir, Db) {
        let temp = tempfile::tempdir().unwrap();
        let db = Db::open(temp.path().join("hub.sqlite")).unwrap();
        (temp, db)
    }

    #[test]
    fn creates_customer_and_site_and_lists_them() {
        let (_temp, db) = open_temp();
        let customer = db.create_customer("Acme").unwrap();
        let site = db.create_site(customer.id, "sede-1").unwrap();

        assert_eq!(db.list_customers().unwrap().len(), 1);
        let sites = db.list_sites().unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].id, site.id);
        assert_eq!(sites[0].status, "unknown");
    }

    #[test]
    fn agent_token_authenticates_its_site_until_revoked() {
        let (_temp, db) = open_temp();
        let customer = db.create_customer("Acme").unwrap();
        let site = db.create_site(customer.id, "sede-1").unwrap();
        let token_id = db.create_agent_token(site.id, "hash-abc", 100).unwrap();

        let found = db.site_for_agent_token("hash-abc").unwrap().unwrap();
        assert_eq!(found.id, site.id);

        assert!(db.revoke_agent_token(site.id, token_id, 200).unwrap());
        assert!(db.site_for_agent_token("hash-abc").unwrap().is_none());
        // Un secondo tentativo di revoca non trova più nulla da revocare.
        assert!(!db.revoke_agent_token(site.id, token_id, 300).unwrap());
    }

    #[test]
    fn heartbeat_marks_site_online_and_records_timestamp() {
        let (_temp, db) = open_temp();
        let customer = db.create_customer("Acme").unwrap();
        let site = db.create_site(customer.id, "sede-1").unwrap();

        db.record_heartbeat(site.id, 1_000).unwrap();
        let refreshed = db.get_site(site.id).unwrap().unwrap();
        assert_eq!(refreshed.status, "online");
        assert_eq!(refreshed.last_heartbeat_at, Some(1_000));
    }

    #[test]
    fn sites_overdue_only_returns_stale_non_offline_sites() {
        let (_temp, db) = open_temp();
        let customer = db.create_customer("Acme").unwrap();
        let stale = db.create_site(customer.id, "stale").unwrap();
        let fresh = db.create_site(customer.id, "fresh").unwrap();
        let never_seen = db.create_site(customer.id, "never-seen").unwrap();

        db.record_heartbeat(stale.id, 1_000).unwrap();
        db.record_heartbeat(fresh.id, 5_000).unwrap();
        let _ = never_seen; // nessun heartbeat: last_heartbeat_at resta NULL

        let overdue = db.sites_overdue(2_000).unwrap();
        assert_eq!(overdue.len(), 1);
        assert_eq!(overdue[0].id, stale.id);

        db.mark_site_offline(stale.id).unwrap();
        assert!(db.sites_overdue(2_000).unwrap().is_empty());
    }

    #[test]
    fn refresh_token_rotates_and_rejects_reuse_or_expiry() {
        let (_temp, db) = open_temp();
        let user_id = db.create_user("admin", "hash", Role::Admin).unwrap();
        db.store_refresh_token(user_id, "token-hash", 100, 1_000)
            .unwrap();

        let (id, role) = db
            .consume_refresh_token("token-hash", 200)
            .unwrap()
            .unwrap();
        assert_eq!(id, user_id);
        assert_eq!(role, Role::Admin);

        // Riuso dello stesso refresh token: già revocato dalla rotazione.
        assert!(db
            .consume_refresh_token("token-hash", 300)
            .unwrap()
            .is_none());

        db.store_refresh_token(user_id, "expiring", 100, 500)
            .unwrap();
        assert!(db.consume_refresh_token("expiring", 999).unwrap().is_none());
    }

    #[test]
    fn records_execution_and_updates_site_last_event() {
        let (_temp, db) = open_temp();
        let customer = db.create_customer("Acme").unwrap();
        let site = db.create_site(customer.id, "sede-1").unwrap();

        db.record_execution(&NewExecution {
            site_id: site.id,
            job: Some("documents"),
            kind: "success",
            detail: None,
            bytes: Some(1024),
            files: Some(3),
            checksum: Some("abc"),
            received_at: 42,
        })
        .unwrap();

        let executions = db.list_executions_for_site(site.id, 10).unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].job.as_deref(), Some("documents"));
        assert_eq!(
            db.get_site(site.id).unwrap().unwrap().last_event_at,
            Some(42)
        );
    }

    fn db_with_site() -> (Db, i64) {
        let temp = tempfile::tempdir().unwrap();
        let db = Db::open(temp.path().join("hub.sqlite")).unwrap();
        std::mem::forget(temp);
        let customer = db.create_customer("Acme").unwrap();
        let site = db.create_site(customer.id, "sede-1").unwrap();
        (db, site.id)
    }

    #[test]
    fn command_lifecycle_pending_delivered_done() {
        let (db, site) = db_with_site();
        let created = db
            .create_command(site, "run", "documents", "admin", 1000)
            .unwrap()
            .expect("creato");
        assert_eq!(created.status, "pending");

        let claimed = db.claim_commands(site, 1005).unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].status, "delivered");
        // Un secondo poll non riceve più lo stesso comando.
        assert!(db.claim_commands(site, 1006).unwrap().is_empty());

        assert!(db
            .finish_command(site, created.id, true, Some("1 file"), 1010)
            .unwrap());
        // L'esito si registra una volta sola.
        assert!(!db
            .finish_command(site, created.id, true, None, 1011)
            .unwrap());

        let listed = db.list_commands(site, 10, 1012).unwrap();
        assert_eq!(listed[0].status, "done");
        assert_eq!(listed[0].detail.as_deref(), Some("1 file"));
        assert_eq!(listed[0].created_by, "admin");
    }

    #[test]
    fn duplicate_active_command_is_refused_until_finished() {
        let (db, site) = db_with_site();
        let first = db
            .create_command(site, "run", "documents", "admin", 1000)
            .unwrap()
            .unwrap();
        assert!(db
            .create_command(site, "run", "documents", "admin", 1001)
            .unwrap()
            .is_none());
        // Azione o job diversi non sono duplicati.
        assert!(db
            .create_command(site, "verify", "documents", "admin", 1001)
            .unwrap()
            .is_some());
        assert!(db
            .create_command(site, "run", "database", "admin", 1001)
            .unwrap()
            .is_some());

        db.claim_commands(site, 1002).unwrap();
        db.finish_command(site, first.id, false, Some("boom"), 1003)
            .unwrap();
        assert!(db
            .create_command(site, "run", "documents", "admin", 1004)
            .unwrap()
            .is_some());
    }

    #[test]
    fn stale_pending_command_expires_and_is_never_delivered() {
        let (db, site) = db_with_site();
        db.create_command(site, "run", "documents", "admin", 1000)
            .unwrap();
        let late = 1000 + PENDING_TTL_SECONDS + 1;
        assert!(db.claim_commands(site, late).unwrap().is_empty());
        assert_eq!(
            db.list_commands(site, 10, late).unwrap()[0].status,
            "expired"
        );
    }

    #[test]
    fn delivered_command_without_result_eventually_fails() {
        let (db, site) = db_with_site();
        db.create_command(site, "run", "documents", "admin", 1000)
            .unwrap();
        db.claim_commands(site, 1001).unwrap();
        let late = 1001 + DELIVERED_TIMEOUT_SECONDS + 1;
        let listed = db.list_commands(site, 10, late).unwrap();
        assert_eq!(listed[0].status, "failed");
        // Non blocca una nuova richiesta.
        assert!(db
            .create_command(site, "run", "documents", "admin", late)
            .unwrap()
            .is_some());
    }

    #[test]
    fn commands_are_scoped_to_their_site() {
        let (db, site) = db_with_site();
        let other = db
            .create_site(db.list_customers().unwrap()[0].id, "sede-2")
            .unwrap();
        let cmd = db
            .create_command(site, "run", "documents", "admin", 1000)
            .unwrap()
            .unwrap();
        assert!(db.claim_commands(other.id, 1001).unwrap().is_empty());
        db.claim_commands(site, 1001).unwrap();
        // Un altro sito non può chiudere un comando non suo.
        assert!(!db
            .finish_command(other.id, cmd.id, true, None, 1002)
            .unwrap());
    }
}
