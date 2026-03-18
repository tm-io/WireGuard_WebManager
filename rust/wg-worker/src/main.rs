//! WireGuard 特権操作用 Worker（root で systemd から起動）。
//! UNIX ドメインソケットで JSON 1 行プロトコルを受け付け、wg 操作のみ実行する。

use serde_json::Value;
use std::path::Path;
use std::process::Command;
use wg_common::config::Settings;
use wg_common::worker_protocol::PeerStat;
#[cfg(unix)]
use {
    nix::unistd::{chown, Gid, Uid, User},
    std::os::unix::fs::PermissionsExt,
};

fn load_config() -> Result<Settings, String> {
    let path = std::env::var("CONFIG_PATH")
        .ok()
        .filter(|p| Path::new(p).is_file())
        .map(|p| Path::new(&p).to_path_buf())
        .unwrap_or_else(|| Path::new(wg_common::config::DEFAULT_CONFIG_PATH).to_path_buf());

    Settings::load(Some(path.as_path()))
}

fn run_wg(args: &[&str]) -> Result<(String, String, i32), ()> {
    let out = Command::new("wg").args(args).output().map_err(|_| ())?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Ok((stdout, stderr, out.status.code().unwrap_or(-1)))
}

fn handle_get_public_key(interface: &str) -> Value {
    match run_wg(&["show", interface, "public-key"]) {
        Ok((out, err, 0)) => serde_json::json!({ "ok": true, "public_key": out }),
        Ok((_, err, code)) => serde_json::json!({ "ok": false, "error": if err.trim().is_empty() { format!("exit {}", code) } else { err } }),
        Err(()) => serde_json::json!({ "ok": false, "error": "wg command failed" }),
    }
}

fn handle_get_peer_stats(interface: &str) -> Value {
    let (out, err, code) = match run_wg(&["show", interface, "dump"]) {
        Ok(x) => x,
        Err(()) => return serde_json::json!({ "ok": false, "error": "wg command failed" }),
    };
    if code != 0 {
        return serde_json::json!({ "ok": false, "error": if err.trim().is_empty() { format!("exit {}", code) } else { err } });
    }
    let lines: Vec<&str> = out.lines().collect();
    if lines.is_empty() {
        return serde_json::json!({ "ok": true, "peers": [] });
    }
    let mut peers = Vec::new();
    for line in lines.iter().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 8 {
            continue;
        }
        let latest_handshake = cols[4].parse::<u64>().ok().filter(|&x| x != 0);
        let rx_bytes = cols[5].parse().unwrap_or(0u64);
        let tx_bytes = cols[6].parse().unwrap_or(0u64);
        peers.push(PeerStat {
            public_key: cols[0].to_string(),
            endpoint: if cols[2].is_empty() { None } else { Some(cols[2].to_string()) },
            allowed_ips: cols[3].split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
            latest_handshake,
            rx_bytes,
            tx_bytes,
        });
    }
    serde_json::json!({ "ok": true, "peers": peers })
}

fn handle_peer_set(interface: &str, public_key: &str, allowed_ips: &[String], preshared_key: Option<&str>) -> Value {
    if public_key.is_empty() || allowed_ips.is_empty() {
        return serde_json::json!({ "ok": false, "error": "public_key and allowed_ips required" });
    }
    let mut args: Vec<String> = vec![
        "set".into(),
        interface.into(),
        "peer".into(),
        public_key.into(),
        "allowed-ips".into(),
        allowed_ips.join(","),
    ];
    let mut psk_path: Option<std::path::PathBuf> = None;
    if let Some(psk) = preshared_key {
        let mut tmp = match tempfile::NamedTempFile::new() {
            Ok(t) => t,
            Err(_) => return serde_json::json!({ "ok": false, "error": "preshared-key temp file failed" }),
        };
        use std::io::Write;
        let _ = tmp.write_all(psk.as_bytes());
        let (_, path_buf) = match tmp.keep() {
            Ok(p) => p,
            Err(_) => return serde_json::json!({ "ok": false, "error": "preshared-key temp file keep failed" }),
        };
        args.push("preshared-key".into());
        args.push(path_buf.to_string_lossy().into_owned());
        psk_path = Some(path_buf);
    }
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = run_wg(&args_ref);
    if let Some(p) = psk_path.as_ref() {
        let _ = std::fs::remove_file(p);
    }
    match result {
        Ok((_, _, 0)) => serde_json::json!({ "ok": true }),
        Ok((_, err, code)) => serde_json::json!({ "ok": false, "error": if err.trim().is_empty() { format!("exit {}", code) } else { err } }),
        Err(()) => serde_json::json!({ "ok": false, "error": "wg command failed" }),
    }
}

fn handle_peer_remove(interface: &str, public_key: &str) -> Value {
    if public_key.is_empty() {
        return serde_json::json!({ "ok": false, "error": "public_key required" });
    }
    match run_wg(&["set", interface, "peer", public_key, "remove"]) {
        Ok((_, _, 0)) => serde_json::json!({ "ok": true }),
        Ok((_, err, code)) => serde_json::json!({ "ok": false, "error": if err.trim().is_empty() { format!("exit {}", code) } else { err } }),
        Err(()) => serde_json::json!({ "ok": false, "error": "wg command failed" }),
    }
}

fn handle_request(interface: &str, req: &Value) -> Value {
    let cmd = req.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
    match cmd {
        "get_public_key" => handle_get_public_key(interface),
        "get_peer_stats" => handle_get_peer_stats(interface),
        "peer_set" => {
            let pk = req.get("public_key").and_then(|v| v.as_str()).unwrap_or("");
            let allowed_ips: Vec<String> = req.get("allowed_ips")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let psk = req.get("preshared_key").and_then(|v| v.as_str());
            handle_peer_set(interface, pk, &allowed_ips, psk)
        }
        "peer_remove" => {
            let pk = req.get("public_key").and_then(|v| v.as_str()).unwrap_or("");
            handle_peer_remove(interface, pk)
        }
        _ => serde_json::json!({ "ok": false, "error": format!("unknown cmd: {}", cmd) }),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        if nix::unistd::geteuid().as_raw() != 0 {
            eprintln!("wg-worker must be run as root.");
            std::process::exit(1);
        }
    }

    let settings = load_config().map_err(|e| {
        eprintln!("config: {}", e);
        e
    })?;
    let socket_path = settings.paths.wg_worker_socket.trim();
    let socket_path = if socket_path.is_empty() {
        "/var/run/wg-manager.sock"
    } else {
        socket_path
    };
    let interface = settings.wireguard.interface.as_str();
    let socket_owner = settings.paths.socket_owner.trim();
    let socket_owner = if socket_owner.is_empty() { "wgwm" } else { socket_owner };

    let socket_path = std::path::Path::new(socket_path);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = std::os::unix::net::UnixListener::bind(socket_path)?;
    #[cfg(unix)]
    {
        // Python 版と同じ: chmod 660, chown を socket_owner に設定（存在しない場合はスキップ）
        let meta = std::fs::metadata(socket_path)?;
        let mut perms = meta.permissions();
        perms.set_mode(0o660);
        std::fs::set_permissions(socket_path, perms)?;

        match User::from_name(socket_owner).map_err(|e| format!("lookup user: {e}"))? {
            Some(u) => {
                let uid = Uid::from_raw(u.uid.as_raw());
                let gid = Gid::from_raw(u.gid.as_raw());
                if let Err(e) = chown(socket_path, Some(uid), Some(gid)) {
                    eprintln!("wg-worker: chown({socket_owner}) failed: {e}");
                }
            }
            None => {
                eprintln!("wg-worker: user {socket_owner:?} not found; socket ownership not changed.");
            }
        }
    }

    eprintln!("Listening on {} (interface={})", socket_path.display(), interface);

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut buf = Vec::new();
        let mut one = [0u8; 1];
        while stream.read(&mut one)? == 1 {
            buf.push(one[0]);
            if one[0] == b'\n' {
                break;
            }
        }
        let line = String::from_utf8_lossy(&buf).trim().to_string();
        let response = if line.is_empty() {
            serde_json::json!({ "ok": false, "error": "empty request" })
        } else {
            let req: Value = serde_json::from_str(&line).unwrap_or(serde_json::json!({ "cmd": "" }));
            handle_request(interface, &req)
        };
        let out = serde_json::to_string(&response).unwrap_or_default() + "\n";
        let _ = stream.write_all(out.as_bytes());
    }
    Ok(())
}

use std::io::{Read, Write};
