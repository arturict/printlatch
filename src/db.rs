use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthenticatedClient, ClientKind};

#[derive(Clone, Debug)]
pub struct Database {
    path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PairingRecord {
    pub origin: String,
    pub name: String,
    pub reuse_client: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    pub id: String,
    pub client_id: String,
    pub printer_id: String,
    pub state: String,
    pub mode: String,
    pub copies: u8,
    pub page_count: u32,
    pub byte_count: u64,
    pub sha256: String,
    pub attempts: u8,
    pub detail: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct NewJob<'a> {
    pub id: &'a str,
    pub client_id: &'a str,
    pub printer_id: &'a str,
    pub state: &'a str,
    pub mode: &'a str,
    pub copies: u8,
    pub page_count: u32,
    pub byte_count: u64,
    pub sha256: &'a str,
    pub file_path: &'a str,
}

impl Database {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let db = Self { path: path.into() };
        db.initialize()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path)
            .with_context(|| format!("could not open {}", self.path.display()))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(connection)
    }

    fn initialize(&self) -> Result<()> {
        let connection = self.connect()?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS clients (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL CHECK (kind IN ('browser', 'local')),
                origin TEXT,
                stable_key TEXT,
                token_hash TEXT NOT NULL UNIQUE,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                revoked_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS pairing_codes (
                code_hash TEXT PRIMARY KEY,
                origin TEXT NOT NULL,
                name TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                consumed_at INTEGER,
                instance_session TEXT,
                reuse_client INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS instance_identity (
                slot INTEGER PRIMARY KEY CHECK (slot = 1),
                secret TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL REFERENCES clients(id),
                printer_id TEXT NOT NULL,
                state TEXT NOT NULL,
                mode TEXT NOT NULL CHECK (mode IN ('preview', 'print')),
                copies INTEGER NOT NULL CHECK (copies BETWEEN 1 AND 10),
                page_count INTEGER NOT NULL,
                byte_count INTEGER NOT NULL,
                sha256 TEXT NOT NULL,
                file_path TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                detail TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS jobs_client_created
                ON jobs(client_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS jobs_queue
                ON jobs(state, created_at);
            ",
        )?;
        let clients_have_stable_key: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('clients')
                WHERE name = 'stable_key'
            )",
            [],
            |row| row.get(0),
        )?;
        if !clients_have_stable_key {
            connection.execute("ALTER TABLE clients ADD COLUMN stable_key TEXT", [])?;
        }
        connection.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS clients_active_stable_key
             ON clients(stable_key)
             WHERE stable_key IS NOT NULL AND revoked_at IS NULL",
            [],
        )?;
        let pairing_has_session: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('pairing_codes')
                WHERE name = 'instance_session'
            )",
            [],
            |row| row.get(0),
        )?;
        if !pairing_has_session {
            connection.execute(
                "ALTER TABLE pairing_codes ADD COLUMN instance_session TEXT",
                [],
            )?;
        }
        let pairing_has_reuse_client: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('pairing_codes')
                WHERE name = 'reuse_client'
            )",
            [],
            |row| row.get(0),
        )?;
        if !pairing_has_reuse_client {
            connection.execute(
                "ALTER TABLE pairing_codes
                 ADD COLUMN reuse_client INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    pub fn recover_interrupted_jobs(&self) -> Result<usize> {
        let now = Utc::now().timestamp();
        Ok(self.connect()?.execute(
            "UPDATE jobs
             SET state = 'unknown',
                 detail = 'Agent restarted while the OS print submission was in progress. Retry manually to avoid an accidental duplicate.',
                 updated_at = ?1
             WHERE state = 'printing'",
            [now],
        )?)
    }

    pub fn insert_pairing_code(
        &self,
        code_hash: &str,
        origin: &str,
        name: &str,
        expires_at: i64,
        instance_session: Option<&str>,
        reuse_client: bool,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM pairing_codes WHERE expires_at < ?1 OR consumed_at IS NOT NULL",
            [now],
        )?;
        transaction.execute(
            "INSERT INTO pairing_codes
             (code_hash, origin, name, expires_at, instance_session, reuse_client)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                code_hash,
                origin,
                name,
                expires_at,
                instance_session,
                reuse_client
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn consume_pairing_code(
        &self,
        code_hash: &str,
        origin: &str,
        now: i64,
        instance_session: &str,
    ) -> Result<Option<PairingRecord>> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let record = transaction
            .query_row(
                "SELECT origin, name, reuse_client FROM pairing_codes
                 WHERE code_hash = ?1 AND origin = ?2 AND expires_at >= ?3
                   AND consumed_at IS NULL
                   AND (instance_session IS NULL OR instance_session = ?4)",
                params![code_hash, origin, now, instance_session],
                |row| {
                    Ok(PairingRecord {
                        origin: row.get(0)?,
                        name: row.get(1)?,
                        reuse_client: row.get(2)?,
                    })
                },
            )
            .optional()?;
        if record.is_some() {
            transaction.execute(
                "UPDATE pairing_codes SET consumed_at = ?3
                 WHERE code_hash = ?1 AND origin = ?2 AND consumed_at IS NULL",
                params![code_hash, origin, now],
            )?;
        }
        transaction.commit()?;
        Ok(record)
    }

    pub fn get_or_create_instance_secret(&self, candidate: &str) -> Result<String> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO instance_identity (slot, secret) VALUES (1, ?1)",
            [candidate],
        )?;
        let secret = transaction.query_row(
            "SELECT secret FROM instance_identity WHERE slot = 1",
            [],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(secret)
    }

    pub fn insert_client(
        &self,
        id: &str,
        name: &str,
        kind: ClientKind,
        origin: Option<&str>,
        token_hash: &str,
        expires_at: i64,
    ) -> Result<()> {
        self.connect()?.execute(
            "INSERT INTO clients
             (id, name, kind, origin, token_hash, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                name,
                kind.as_str(),
                origin,
                token_hash,
                expires_at,
                Utc::now().timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn rotate_or_insert_stable_browser_client(
        &self,
        new_id: &str,
        stable_key: &str,
        name: &str,
        origin: &str,
        token_hash: &str,
        expires_at: i64,
    ) -> Result<String> {
        let now = Utc::now().timestamp();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let existing_id = transaction
            .query_row(
                "SELECT id FROM clients
                 WHERE kind = 'browser' AND stable_key = ?1 AND revoked_at IS NULL
                 ORDER BY created_at DESC LIMIT 1",
                [stable_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let client_id = if let Some(existing_id) = existing_id {
            let changed = transaction.execute(
                "UPDATE clients
                 SET name = ?1, origin = ?2, token_hash = ?3, expires_at = ?4
                 WHERE id = ?5 AND stable_key = ?6 AND revoked_at IS NULL",
                params![
                    name,
                    origin,
                    token_hash,
                    expires_at,
                    existing_id,
                    stable_key
                ],
            )?;
            anyhow::ensure!(
                changed == 1,
                "stable browser client changed during token rotation"
            );
            existing_id
        } else {
            transaction.execute(
                "INSERT INTO clients
                 (id, name, kind, origin, stable_key, token_hash, expires_at, created_at)
                 VALUES (?1, ?2, 'browser', ?3, ?4, ?5, ?6, ?7)",
                params![
                    new_id, name, origin, stable_key, token_hash, expires_at, now
                ],
            )?;
            new_id.to_owned()
        };
        transaction.commit()?;
        Ok(client_id)
    }

    pub fn get_client(&self, id: &str) -> Result<Option<AuthenticatedClient>> {
        self.connect()?
            .query_row(
                "SELECT id, name, kind, origin FROM clients
                 WHERE id = ?1 AND revoked_at IS NULL",
                [id],
                client_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_client_by_token_hash(
        &self,
        token_hash: &str,
        now: i64,
    ) -> Result<Option<AuthenticatedClient>> {
        self.connect()?
            .query_row(
                "SELECT id, name, kind, origin FROM clients
                 WHERE token_hash = ?1 AND expires_at >= ?2 AND revoked_at IS NULL",
                params![token_hash, now],
                client_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_clients(&self) -> Result<Vec<(AuthenticatedClient, i64, bool)>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, name, kind, origin, expires_at, revoked_at IS NOT NULL
             FROM clients ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                AuthenticatedClient {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: ClientKind::try_from(row.get::<_, String>(2)?.as_str()).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                error.into(),
                            )
                        },
                    )?,
                    origin: row.get(3)?,
                },
                row.get(4)?,
                row.get(5)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn rotate_client_token(&self, id: &str, token_hash: &str, expires_at: i64) -> Result<()> {
        let changed = self.connect()?.execute(
            "UPDATE clients SET token_hash = ?2, expires_at = ?3
             WHERE id = ?1 AND revoked_at IS NULL",
            params![id, token_hash, expires_at],
        )?;
        anyhow::ensure!(changed == 1, "client does not exist");
        Ok(())
    }

    pub fn revoke_client(&self, id: &str) -> Result<()> {
        let changed = self.connect()?.execute(
            "UPDATE clients SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            params![id, Utc::now().timestamp()],
        )?;
        anyhow::ensure!(changed == 1, "client does not exist or is already revoked");
        Ok(())
    }

    pub fn insert_job(&self, job: &NewJob<'_>) -> Result<()> {
        let now = Utc::now().timestamp();
        let byte_count = i64::try_from(job.byte_count).context("job byte count is out of range")?;
        self.connect()?.execute(
            "INSERT INTO jobs
             (id, client_id, printer_id, state, mode, copies, page_count, byte_count,
              sha256, file_path, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                job.id,
                job.client_id,
                job.printer_id,
                job.state,
                job.mode,
                job.copies,
                job.page_count,
                byte_count,
                job.sha256,
                job.file_path,
                now
            ],
        )?;
        Ok(())
    }

    pub fn get_job_for_client(&self, id: &str, client_id: &str) -> Result<Option<Job>> {
        self.connect()?
            .query_row(
                "SELECT id, client_id, printer_id, state, mode, copies, page_count,
                        byte_count, sha256, attempts, detail, created_at, updated_at
                 FROM jobs WHERE id = ?1 AND client_id = ?2",
                params![id, client_id],
                job_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_job_file_for_client(
        &self,
        id: &str,
        client_id: &str,
    ) -> Result<Option<(Job, PathBuf)>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, client_id, printer_id, state, mode, copies, page_count,
                        byte_count, sha256, attempts, detail, created_at, updated_at,
                        file_path
                 FROM jobs WHERE id = ?1 AND client_id = ?2",
                params![id, client_id],
                |row| Ok((job_from_row(row)?, PathBuf::from(row.get::<_, String>(13)?))),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_jobs_for_client(&self, client_id: &str, limit: u16) -> Result<Vec<Job>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, client_id, printer_id, state, mode, copies, page_count,
                    byte_count, sha256, attempts, detail, created_at, updated_at
             FROM jobs WHERE client_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![client_id, limit], job_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn claim_next_job(&self) -> Result<Option<(Job, PathBuf)>> {
        let now = Utc::now().timestamp();
        self.connect()?
            .query_row(
                "UPDATE jobs
                 SET state = 'printing', attempts = attempts + 1, updated_at = ?1
                 WHERE id = (
                     SELECT id FROM jobs
                     WHERE state = 'queued'
                     ORDER BY created_at
                     LIMIT 1
                 )
                 AND state = 'queued'
                 RETURNING id, client_id, printer_id, state, mode, copies, page_count,
                           byte_count, sha256, attempts, detail, created_at, updated_at,
                           file_path",
                [now],
                |row| Ok((job_from_row(row)?, PathBuf::from(row.get::<_, String>(13)?))),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn finish_job(&self, id: &str, state: &str, detail: &str) -> Result<()> {
        self.connect()?.execute(
            "UPDATE jobs SET state = ?2, detail = ?3, updated_at = ?4
             WHERE id = ?1 AND state = 'printing'",
            params![id, state, detail, Utc::now().timestamp()],
        )?;
        Ok(())
    }

    pub fn cancel_job(&self, id: &str, client_id: &str) -> Result<bool> {
        Ok(self.connect()?.execute(
            "UPDATE jobs SET state = 'canceled', detail = 'Canceled by client',
                    updated_at = ?3
             WHERE id = ?1 AND client_id = ?2 AND state = 'queued'",
            params![id, client_id, Utc::now().timestamp()],
        )? == 1)
    }

    pub fn retry_job(&self, id: &str, client_id: &str) -> Result<bool> {
        Ok(self.connect()?.execute(
            "UPDATE jobs SET state = 'queued', detail = NULL, updated_at = ?3
             WHERE id = ?1 AND client_id = ?2
               AND state IN ('failed', 'unknown') AND attempts < 3",
            params![id, client_id, Utc::now().timestamp()],
        )? == 1)
    }

    pub fn job_state(&self, id: &str, client_id: &str) -> Result<Option<(String, u8)>> {
        self.connect()?
            .query_row(
                "SELECT state, attempts FROM jobs WHERE id = ?1 AND client_id = ?2",
                params![id, client_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }
}

fn client_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthenticatedClient> {
    let kind = row.get::<_, String>(2)?;
    Ok(AuthenticatedClient {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: ClientKind::try_from(kind.as_str()).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, error.into())
        })?,
        origin: row.get(3)?,
    })
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let byte_count = row.get::<_, i64>(7)?;
    Ok(Job {
        id: row.get(0)?,
        client_id: row.get(1)?,
        printer_id: row.get(2)?,
        state: row.get(3)?,
        mode: row.get(4)?,
        copies: row.get(5)?,
        page_count: row.get(6)?,
        byte_count: u64::try_from(byte_count).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Integer,
                error.into(),
            )
        })?,
        sha256: row.get(8)?,
        attempts: row.get(9)?,
        detail: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_existing_pairing_codes_to_explicit_non_reuse() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("migration.sqlite3");
        let connection = Connection::open(&path).expect("legacy database");
        connection
            .execute_batch(
                "
                CREATE TABLE pairing_codes (
                    code_hash TEXT PRIMARY KEY,
                    origin TEXT NOT NULL,
                    name TEXT NOT NULL,
                    expires_at INTEGER NOT NULL,
                    consumed_at INTEGER
                );
                INSERT INTO pairing_codes
                    (code_hash, origin, name, expires_at, consumed_at)
                VALUES ('legacy', 'https://app.example', 'Browser app', 1, NULL);
                CREATE TABLE clients (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK (kind IN ('browser', 'local')),
                    origin TEXT,
                    token_hash TEXT NOT NULL UNIQUE,
                    expires_at INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    revoked_at INTEGER
                );
                INSERT INTO clients
                    (id, name, kind, origin, token_hash, expires_at, created_at)
                VALUES (
                    'legacy-client',
                    'PrintLatch dashboard',
                    'browser',
                    'http://127.0.0.1:32191',
                    'legacy-token',
                    1,
                    1
                );
                ",
            )
            .expect("legacy pairing table");
        drop(connection);

        let db = Database::open(&path).expect("migrated database");
        let connection = db.connect().expect("migrated connection");
        let (session, reuse_client): (Option<String>, bool) = connection
            .query_row(
                "SELECT instance_session, reuse_client
                 FROM pairing_codes WHERE code_hash = 'legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("migrated pairing record");
        assert!(session.is_none());
        assert!(!reuse_client);
        let stable_key: Option<String> = connection
            .query_row(
                "SELECT stable_key FROM clients WHERE id = 'legacy-client'",
                [],
                |row| row.get(0),
            )
            .expect("migrated client record");
        assert!(stable_key.is_none());
    }
}
