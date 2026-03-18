//! config.yaml の読み書き用構造体（Python の Settings 相当）

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Linux デーモンとしてインストールされた場合のデフォルト設定ファイルパス
pub const DEFAULT_CONFIG_PATH: &str = "/etc/wireguard-webmanager/config.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub auth_username: String,
    pub auth_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub db_path: String,
    pub wg_conf_dir: String,
    pub wg_worker_socket: String,
    pub socket_owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub interface: String,
    pub server_endpoint: String,
    pub listen_port: u16,
    pub client_ip_range: String,
    pub client_dns: String,
    pub persistent_keepalive: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub app: AppConfig,
    pub paths: PathsConfig,
    pub wireguard: WireGuardConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            auth_username: "admin".to_string(),
            auth_password: "password123".to_string(),
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            db_path: "/var/lib/wireguard-webmanager/wg-manager.db".to_string(),
            wg_conf_dir: "/etc/wireguard".to_string(),
            wg_worker_socket: "/run/wg-manager.sock".to_string(),
            socket_owner: "wgwm".to_string(),
        }
    }
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            interface: "wg0".to_string(),
            server_endpoint: "127.0.0.1".to_string(),
            listen_port: 51820,
            client_ip_range: "10.8.0.0/24".to_string(),
            client_dns: "1.1.1.1, 8.8.8.8".to_string(),
            persistent_keepalive: 25,
        }
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    app: Option<AppConfig>,
    paths: Option<PathsConfig>,
    wireguard: Option<WireGuardConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            app: AppConfig::default(),
            paths: PathsConfig::default(),
            wireguard: WireGuardConfig::default(),
        }
    }
}

impl Settings {
    /// config.yaml を読み込む。パス未指定時は `/etc/wireguard-webmanager/config.yaml`。
    /// ファイルが存在しない場合はデフォルト設定で自動生成する。
    pub fn load(config_path: Option<&Path>) -> Result<Self, String> {
        let default_path = Path::new(DEFAULT_CONFIG_PATH);
        let path = config_path.unwrap_or(default_path);

        if !path.exists() {
            let defaults = Settings::default();
            // 親ディレクトリを作成してからデフォルト設定を書き出す
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("config dir create: {}", e))?;
            }
            let yaml = serde_yaml::to_string(&defaults)
                .map_err(|e| format!("config serialize: {}", e))?;
            fs::write(path, yaml).map_err(|e| format!("config write: {}", e))?;
            eprintln!("config: {} が存在しないためデフォルト設定で自動生成しました", path.display());
            return Ok(defaults);
        }

        let s = fs::read_to_string(path).map_err(|e| format!("config read: {}", e))?;
        let raw: ConfigFile = serde_yaml::from_str(&s).map_err(|e| format!("config parse: {}", e))?;
        Ok(Settings {
            app: raw.app.unwrap_or_default(),
            paths: raw.paths.unwrap_or_default(),
            wireguard: raw.wireguard.unwrap_or_default(),
        })
    }

    /// 現在の設定を YAML として保存（設定画面用）。raw は app/paths/wireguard をマージした全体。
    pub fn save(raw: &impl Serialize, config_path: Option<&Path>) -> Result<(), String> {
        let path = config_path.unwrap_or(Path::new(DEFAULT_CONFIG_PATH));
        let s = serde_yaml::to_string(raw).map_err(|e| format!("config serialize: {}", e))?;
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, s).map_err(|e| format!("config write: {}", e))?;
        Ok(())
    }
}
