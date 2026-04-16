//! SQLite による Peer 永続化（Python の app/core/db + app/repositories/peers 相当）

use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct AclRule {
    pub id: i64,
    pub peer_id: i64,
    pub action: String,      // "allow" | "deny"
    pub target_cidr: String,
    pub protocol: String,    // "any" | "tcp" | "udp" | "icmp"
    pub port_range: String,  // "" | "80" | "80-443"（tcp/udp のみ有効）
    pub description: String,
    pub priority: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct TrafficSnapshot {
    pub recorded_at: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
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
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

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
            );
            CREATE TABLE IF NOT EXISTS peer_traffic_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                peer_id INTEGER NOT NULL REFERENCES peers(id) ON DELETE CASCADE,
                recorded_at TEXT NOT NULL,
                rx_bytes INTEGER NOT NULL DEFAULT 0,
                tx_bytes INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_traffic_log_peer_time
                ON peer_traffic_log(peer_id, recorded_at);
            CREATE TABLE IF NOT EXISTS peer_acl_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                peer_id INTEGER NOT NULL REFERENCES peers(id) ON DELETE CASCADE,
                action TEXT NOT NULL CHECK(action IN ('allow', 'deny')),
                target_cidr TEXT NOT NULL,
                protocol TEXT NOT NULL DEFAULT 'any',
                port_range TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                priority INTEGER NOT NULL DEFAULT 100,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_acl_peer
                ON peer_acl_rules(peer_id, priority);
            "#,
        ).map_err(|e| e.to_string())?;

        // 既存DBへのマイグレーション: protocol / port_range カラムを追加（なければ）
        let conn = self.conn()?;
        let _ = conn.execute("ALTER TABLE peer_acl_rules ADD COLUMN protocol TEXT NOT NULL DEFAULT 'any'", []);
        let _ = conn.execute("ALTER TABLE peer_acl_rules ADD COLUMN port_range TEXT NOT NULL DEFAULT ''", []);

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

    pub fn update_peer_name(&self, peer_id: i64, name: &str) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute("UPDATE peers SET name = ? WHERE id = ?", params![name, peer_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_traffic_snapshot(&self, peer_id: i64, rx_bytes: u64, tx_bytes: u64) -> Result<(), String> {
        let recorded_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO peer_traffic_log (peer_id, recorded_at, rx_bytes, tx_bytes) VALUES (?, ?, ?, ?)",
            params![peer_id, recorded_at, rx_bytes as i64, tx_bytes as i64],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_traffic_history(&self, peer_id: i64, limit: i64) -> Result<Vec<TrafficSnapshot>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT recorded_at, rx_bytes, tx_bytes FROM peer_traffic_log
             WHERE peer_id = ? ORDER BY recorded_at DESC LIMIT ?",
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![peer_id, limit], |row| {
            Ok(TrafficSnapshot {
                recorded_at: row.get(0)?,
                rx_bytes: row.get::<_, i64>(1)? as u64,
                tx_bytes: row.get::<_, i64>(2)? as u64,
            })
        }).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        // DESC で取得したので reverse して時系列順に
        out.reverse();
        Ok(out)
    }

    // ---- ACL ----

    pub fn list_acl_rules(&self, peer_id: i64) -> Result<Vec<AclRule>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, peer_id, action, target_cidr, protocol, port_range, description, priority, created_at
             FROM peer_acl_rules WHERE peer_id = ? ORDER BY priority ASC, id ASC",
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![peer_id], |row| {
            Ok(AclRule {
                id: row.get(0)?,
                peer_id: row.get(1)?,
                action: row.get(2)?,
                target_cidr: row.get(3)?,
                protocol: row.get(4)?,
                port_range: row.get(5)?,
                description: row.get(6)?,
                priority: row.get(7)?,
                created_at: row.get(8)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_all_acl_rules(&self) -> Result<Vec<AclRule>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, peer_id, action, target_cidr, protocol, port_range, description, priority, created_at
             FROM peer_acl_rules ORDER BY peer_id ASC, priority ASC, id ASC",
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(AclRule {
                id: row.get(0)?,
                peer_id: row.get(1)?,
                action: row.get(2)?,
                target_cidr: row.get(3)?,
                protocol: row.get(4)?,
                port_range: row.get(5)?,
                description: row.get(6)?,
                priority: row.get(7)?,
                created_at: row.get(8)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn create_acl_rule(
        &self,
        peer_id: i64,
        action: &str,
        target_cidr: &str,
        protocol: &str,
        port_range: &str,
        description: &str,
        priority: i64,
    ) -> Result<AclRule, String> {
        let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO peer_acl_rules (peer_id, action, target_cidr, protocol, port_range, description, priority, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![peer_id, action, target_cidr, protocol, port_range, description, priority, created_at],
        ).map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        let mut stmt = conn.prepare(
            "SELECT id, peer_id, action, target_cidr, protocol, port_range, description, priority, created_at
             FROM peer_acl_rules WHERE id = ?",
        ).map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        rows.next().map_err(|e| e.to_string())?
            .map(|row| Ok(AclRule {
                id: row.get(0).map_err(|e: rusqlite::Error| e.to_string())?,
                peer_id: row.get(1).map_err(|e: rusqlite::Error| e.to_string())?,
                action: row.get(2).map_err(|e: rusqlite::Error| e.to_string())?,
                target_cidr: row.get(3).map_err(|e: rusqlite::Error| e.to_string())?,
                protocol: row.get(4).map_err(|e: rusqlite::Error| e.to_string())?,
                port_range: row.get(5).map_err(|e: rusqlite::Error| e.to_string())?,
                description: row.get(6).map_err(|e: rusqlite::Error| e.to_string())?,
                priority: row.get(7).map_err(|e: rusqlite::Error| e.to_string())?,
                created_at: row.get(8).map_err(|e: rusqlite::Error| e.to_string())?,
            }))
            .unwrap_or(Err("inserted rule not found".to_string()))
    }

    pub fn delete_acl_rule(&self, rule_id: i64) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM peer_acl_rules WHERE id = ?", params![rule_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// ピアごとの全 ACL ルールを返す（nftables 適用用）
    pub fn get_peer_acl_rules_for_apply(&self, peer_id: i64) -> Result<Vec<AclRule>, String> {
        self.list_acl_rules(peer_id)
    }

    /// ピアごとに最新 keep_per_peer 件を残してそれ以前を削除（SQLite 3.25+ のウィンドウ関数を使用）
    pub fn prune_traffic_log(&self, keep_per_peer: i64) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM peer_traffic_log WHERE id NOT IN (
                SELECT id FROM (
                    SELECT id, row_number() OVER (PARTITION BY peer_id ORDER BY recorded_at DESC) AS rn
                    FROM peer_traffic_log
                ) WHERE rn <= ?
            )",
            params![keep_per_peer],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
}
