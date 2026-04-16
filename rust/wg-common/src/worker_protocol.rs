//! Worker との JSON 1 行プロトコル（Python と互換）

use serde::{Deserialize, Serialize};

/// ACL ルール（wg-manager → wg-worker 間で受け渡す）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRule {
    pub action: String,      // "allow" | "deny"
    pub target_cidr: String,
    pub protocol: String,    // "any" | "tcp" | "udp" | "icmp"
    pub port_range: String,  // "" | "80" | "80-443"（tcp/udp のみ有効）
    pub priority: i64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum WorkerRequest {
    GetPublicKey,
    GetPeerStats,
    PeerSet {
        public_key: String,
        allowed_ips: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        preshared_key: Option<String>,
    },
    PeerRemove {
        public_key: String,
    },
    UpdateWireGuard,
    /// ピアの ACL を nftables に適用する。rules が空なら当該ピアのルールを削除。
    ApplyAclRules {
        peer_ip: String,
        rules: Vec<AclRule>,
    },
    /// 全ピアの ACL を一括再適用（起動時リストア用）
    ReloadAllAcl {
        peers: Vec<PeerAclEntry>,
    },
}

/// ReloadAllAcl で渡すピアごとのエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAclEntry {
    pub peer_ip: String,
    pub rules: Vec<AclRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peers: Option<Vec<PeerStat>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStat {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub latest_handshake: Option<u64>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}
