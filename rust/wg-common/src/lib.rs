//! 設定と Worker プロトコルの共有定義

pub mod config;
pub mod worker_protocol;

pub use config::{AppConfig, PathsConfig, Settings, WireGuardConfig};
pub use worker_protocol::{PeerStat, WorkerRequest, WorkerResponse};
