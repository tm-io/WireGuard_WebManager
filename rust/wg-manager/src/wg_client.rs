//! Worker への UNIX ソケットクライアント（Python の app/core/wg_worker_client 相当）

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use wg_common::worker_protocol::WorkerResponse;
use wg_common::Settings;

fn request(settings: &Settings, payload: &[u8]) -> Result<WorkerResponse, String> {
    let path = settings.paths.wg_worker_socket.trim();
    if path.is_empty() {
        return Err("wg_worker_socket が設定されていません".to_string());
    }
    let mut stream = UnixStream::connect(path).map_err(|e| format!("Worker 接続失敗: {}", e))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(15))).ok();
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5))).ok();
    stream.write_all(payload).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let mut one = [0u8; 1];
    while stream.read(&mut one).map_err(|e| e.to_string())? == 1 {
        buf.push(one[0]);
        if one[0] == b'\n' {
            break;
        }
    }
    let line = String::from_utf8_lossy(&buf);
    let resp: WorkerResponse = serde_json::from_str(line.trim()).map_err(|e| format!("Worker 応答パース: {}", e))?;
    if !resp.ok {
        let err = resp.error.unwrap_or_else(|| "unknown".to_string());
        return Err(err);
    }
    Ok(resp)
}

pub fn get_server_public_key(settings: &Settings) -> Result<String, String> {
    let payload = br#"{"cmd":"get_public_key"}"#;
    let resp = request(settings, payload)?;
    resp.public_key.ok_or_else(|| "no public_key in response".to_string())
}

pub fn get_peer_stats(settings: &Settings) -> Result<Vec<wg_common::PeerStat>, String> {
    let payload = br#"{"cmd":"get_peer_stats"}"#;
    let resp = request(settings, payload)?;
    Ok(resp.peers.unwrap_or_default())
}

pub fn peer_set(
    settings: &Settings,
    public_key: &str,
    allowed_ips: &[String],
    preshared_key: Option<&str>,
) -> Result<(), String> {
    let mut obj = serde_json::json!({
        "cmd": "peer_set",
        "public_key": public_key,
        "allowed_ips": allowed_ips,
    });
    if let Some(psk) = preshared_key {
        obj["preshared_key"] = serde_json::Value::String(psk.to_string());
    }
    let payload = serde_json::to_vec(&obj).map_err(|e| e.to_string())?;
    let _ = request(settings, &payload)?;
    Ok(())
}

pub fn peer_remove(settings: &Settings, public_key: &str) -> Result<(), String> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "cmd": "peer_remove",
        "public_key": public_key,
    }))
    .map_err(|e| e.to_string())?;
    let _ = request(settings, &payload)?;
    Ok(())
}
