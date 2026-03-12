//! SQLite による Peer 永続化（Python の app/core/db + app/repositories/peers 相当）

use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: i64,
    pub name: String,
    pub public_key: String,
    pub private_key_encrypted: String,
    pub pre_shared_key: Option<String>,
    pub allocated_ip: String,
    pub is_active: bool,
    pub created_at: String,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Ok(Database {
            path: path.to_path_buf(),
        })
    }

    fn conn(&self) -> Result<Connection, String> {
        Connection::open(&self.path).map_err(|e| e.to_string())
    }

    pub fn init(&self) -> Result<(), String> {
        self.conn()?.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS peers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                public_key TEXT NOT NULL UNIQUE,
                private_key_encrypted TEXT NOT NULL,
                pre_shared_key TEXT,
                allocated_ip TEXT NOT NULL UNIQUE,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            )
            "#,
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_peers(&self) -> Result<Vec<Peer>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, public_key, private_key_encrypted, pre_shared_key, allocated_ip, is_active, created_at FROM peers ORDER BY id ASC",
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(Peer {
                id: row.get(0)?,
                name: row.get(1)?,
                public_key: row.get(2)?,
                private_key_encrypted: row.get(3)?,
                pre_shared_key: row.get(4)?,
                allocated_ip: row.get(5)?,
                is_active: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_peer(&self, peer_id: i64) -> Result<Option<Peer>, String> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, public_key, private_key_encrypted, pre_shared_key, allocated_ip, is_active, created_at FROM peers WHERE id = ?",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![peer_id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            return Ok(Some(Peer {
                id: row.get(0).map_err(|e| e.to_string())?,
                name: row.get(1).map_err(|e| e.to_string())?,
                public_key: row.get(2).map_err(|e| e.to_string())?,
                private_key_encrypted: row.get(3).map_err(|e| e.to_string())?,
                pre_shared_key: row.get(4).map_err(|e| e.to_string())?,
                allocated_ip: row.get(5).map_err(|e| e.to_string())?,
                is_active: row.get::<_, i32>(6).map_err(|e| e.to_string())? != 0,
                created_at: row.get(7).map_err(|e| e.to_string())?,
            }));
        }
        Ok(None)
    }

    pub fn list_allocated_ips(&self) -> Result<Vec<String>, String> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT allocated_ip FROM peers WHERE is_active = 1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn create_peer(
        &self,
        name: &str,
        public_key: &str,
        private_key_encrypted: &str,
        pre_shared_key: Option<&str>,
        allocated_ip: &str,
        is_active: bool,
    ) -> Result<Peer, String> {
        let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let conn = self.conn()?;
        conn.execute(
            r#"
            INSERT INTO peers (name, public_key, private_key_encrypted, pre_shared_key, allocated_ip, is_active, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                name,
                public_key,
                private_key_encrypted,
                pre_shared_key,
                allocated_ip,
                if is_active { 1 } else { 0 },
                created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        self.get_peer(id)?
            .ok_or_else(|| "created peer not found".to_string())
    }

    pub fn set_peer_active(&self, peer_id: i64, is_active: bool) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE peers SET is_active = ? WHERE id = ?",
            params![if is_active { 1 } else { 0 }, peer_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_peer(&self, peer_id: i64) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM peers WHERE id = ?", params![peer_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
