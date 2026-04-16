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

    tracing::debug!("設定ファイルを読み込みます: {}", path.display());
    Settings::load(Some(path.as_path()))
}

/// `wg` コマンドを実行し (stdout, stderr, exit_code) を返す。
/// wg が見つからない・起動失敗の場合は Err(診断メッセージ) を返す。
fn run_wg(args: &[&str]) -> Result<(String, String, i32), String> {
    Command::new("wg")
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "wg コマンドが見つかりません。wireguard-tools がインストールされているか確認してください \
                 (apt install wireguard-tools  /  dnf install wireguard-tools)"
                    .to_string()
            } else {
                format!("wg コマンドの起動に失敗しました: {e}")
            }
        })
        .map(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            (stdout, stderr, out.status.code().unwrap_or(-1))
        })
}

fn handle_get_public_key(interface: &str) -> Value {
    match run_wg(&["show", interface, "public-key"]) {
        Ok((out, _err, 0)) => serde_json::json!({ "ok": true, "public_key": out }),
        Ok((_, err, code)) => {
            let msg = if err.trim().is_empty() {
                format!("wg show {interface} public-key: exit {code}")
            } else {
                err
            };
            tracing::error!("get_public_key 失敗 (interface={}): {}", interface, msg);
            serde_json::json!({ "ok": false, "error": msg })
        }
        Err(e) => {
            tracing::error!("get_public_key 実行エラー: {}", e);
            serde_json::json!({ "ok": false, "error": e })
        }
    }
}

fn handle_get_peer_stats(interface: &str) -> Value {
    let (out, err, code) = match run_wg(&["show", interface, "dump"]) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!("get_peer_stats 実行エラー: {}", e);
            return serde_json::json!({ "ok": false, "error": e });
        }
    };
    if code != 0 {
        let msg = if err.trim().is_empty() {
            format!("wg show {interface} dump: exit {code}")
        } else {
            err
        };
        tracing::error!("get_peer_stats 失敗 (interface={}): {}", interface, msg);
        return serde_json::json!({ "ok": false, "error": msg });
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

fn handle_peer_set(
    interface: &str,
    public_key: &str,
    allowed_ips: &[String],
    preshared_key: Option<&str>,
) -> Value {
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
            Err(e) => {
                tracing::error!("PSK 用一時ファイルの作成に失敗しました: {}", e);
                return serde_json::json!({ "ok": false, "error": format!("preshared-key temp file failed: {e}") });
            }
        };
        use std::io::Write;
        let _ = tmp.write_all(psk.as_bytes());
        let (_, path_buf) = match tmp.keep() {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("PSK 用一時ファイルの保持に失敗しました: {}", e);
                return serde_json::json!({ "ok": false, "error": format!("preshared-key temp file keep failed: {e}") });
            }
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
    let peer_short = &public_key[..8.min(public_key.len())];
    match result {
        Ok((_, _, 0)) => {
            tracing::info!("peer_set 成功: peer={}... interface={}", peer_short, interface);
            serde_json::json!({ "ok": true })
        }
        Ok((_, err, code)) => {
            let msg = if err.trim().is_empty() {
                format!("wg set peer: exit {code}")
            } else {
                err
            };
            tracing::error!("peer_set 失敗 (peer={}... interface={}): {}", peer_short, interface, msg);
            serde_json::json!({ "ok": false, "error": msg })
        }
        Err(e) => {
            tracing::error!("peer_set 実行エラー: {}", e);
            serde_json::json!({ "ok": false, "error": e })
        }
    }
}

fn handle_peer_remove(interface: &str, public_key: &str) -> Value {
    if public_key.is_empty() {
        return serde_json::json!({ "ok": false, "error": "public_key required" });
    }
    let peer_short = &public_key[..8.min(public_key.len())];
    match run_wg(&["set", interface, "peer", public_key, "remove"]) {
        Ok((_, _, 0)) => {
            tracing::info!("peer_remove 成功: peer={}... interface={}", peer_short, interface);
            serde_json::json!({ "ok": true })
        }
        Ok((_, err, code)) => {
            let msg = if err.trim().is_empty() {
                format!("wg set peer remove: exit {code}")
            } else {
                err
            };
            tracing::error!("peer_remove 失敗 (peer={}... interface={}): {}", peer_short, interface, msg);
            serde_json::json!({ "ok": false, "error": msg })
        }
        Err(e) => {
            tracing::error!("peer_remove 実行エラー: {}", e);
            serde_json::json!({ "ok": false, "error": e })
        }
    }
}

// ---- nftables ACL ----
//
// wgwm テーブルだけを完全管理する。他のテーブルには一切触れない。
//
// テーブル構造:
//   table inet wgwm {
//     chain forward {
//       type filter hook forward priority 0; policy accept;
//       # ピアごとのルールが priority 昇順で並ぶ
//       ip saddr 10.0.0.2 ip daddr 192.168.1.0/24 drop  comment "wgwm: peer 10.0.0.2 deny"
//       ip saddr 10.0.0.2 ip daddr 10.0.0.0/24   accept comment "wgwm: peer 10.0.0.2 allow"
//     }
//   }

use wg_common::worker_protocol::{AclRule, PeerAclEntry};

/// nft コマンドを実行
fn run_nft(args: &[&str]) -> Result<String, String> {
    Command::new("nft")
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "nft コマンドが見つかりません。nftables がインストールされているか確認してください \
                 (apt install nftables)"
                    .to_string()
            } else {
                format!("nft コマンドの起動に失敗しました: {e}")
            }
        })
        .and_then(|out| {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
            }
        })
}

/// wgwm テーブルが存在するか確認
fn wgwm_table_exists() -> bool {
    run_nft(&["list", "table", "inet", "wgwm"]).is_ok()
}

/// wgwm テーブルを初期化（存在しなければ作成）
fn ensure_wgwm_table() -> Result<(), String> {
    if !wgwm_table_exists() {
        run_nft(&[
            "add", "table", "inet", "wgwm",
        ])?;
        run_nft(&[
            "add", "chain", "inet", "wgwm", "forward",
            "{ type filter hook forward priority 0; policy accept; }",
        ])?;
        tracing::info!("nftables: wgwm テーブルを作成しました");
    }
    Ok(())
}

/// ルール 1 件分の nft add ルール文字列を生成
fn build_nft_rule(peer_ip: &str, rule: &AclRule) -> String {
    let verdict = match rule.action.as_str() {
        "allow" => "accept",
        _       => "drop",
    };

    // プロトコル + ポート範囲の nft 式を組み立て
    let proto_expr = match rule.protocol.as_str() {
        "tcp" | "udp" => {
            let proto = rule.protocol.as_str();
            if rule.port_range.trim().is_empty() {
                // ポート未指定: プロトコルのみ
                format!("{} ", proto)
            } else if rule.port_range.contains('-') {
                // 範囲指定: "80-443" → "tcp dport 80-443"
                format!("{} dport {} ", proto, rule.port_range.trim())
            } else {
                // 単一ポート: "80" → "tcp dport 80"
                format!("{} dport {} ", proto, rule.port_range.trim())
            }
        }
        "icmp" => "meta l4proto icmp ".to_string(),
        _ => String::new(),  // "any" またはその他: プロトコル指定なし
    };

    let desc = if rule.description.is_empty() {
        format!("wgwm: {} {} {}", peer_ip, rule.action, rule.target_cidr)
    } else {
        format!("wgwm: {} {} {} ({})", peer_ip, rule.action, rule.target_cidr, rule.description)
    };
    // comment はダブルクォートで囲む
    format!(
        "ip saddr {} ip daddr {} {}{}comment \"{}\"",
        peer_ip, rule.target_cidr, proto_expr, verdict, desc
    )
}

/// 指定ピアの ACL ルールをすべて削除し、新しいルールを適用する（冪等）
fn apply_peer_acl(peer_ip: &str, rules: &[AclRule]) -> Value {
    if let Err(e) = ensure_wgwm_table() {
        return serde_json::json!({ "ok": false, "error": e });
    }

    // 既存の当該ピアのルールを削除（comment で grep）
    // nft list chain → 行ごとに handle を探して削除
    let chain_dump = match run_nft(&["--handle", "list", "chain", "inet", "wgwm", "forward"]) {
        Ok(s) => s,
        Err(e) => return serde_json::json!({ "ok": false, "error": format!("chain dump 失敗: {}", e) }),
    };

    // "wgwm: <peer_ip> " を含む行の handle を収集して削除
    let search = format!("wgwm: {}", peer_ip);
    let mut handles_to_delete: Vec<String> = Vec::new();
    for line in chain_dump.lines() {
        if line.contains(&search) {
            // "# handle N" の形式で行末に handle が来る
            if let Some(h) = line.split("# handle ").nth(1) {
                let handle = h.trim().to_string();
                if !handle.is_empty() {
                    handles_to_delete.push(handle);
                }
            }
        }
    }
    for handle in &handles_to_delete {
        if let Err(e) = run_nft(&["delete", "rule", "inet", "wgwm", "forward", "handle", handle]) {
            tracing::warn!("nftables: handle {} の削除に失敗: {}", handle, e);
        }
    }

    // 新しいルールを priority 順（昇順）に追加
    let mut sorted_rules = rules.to_vec();
    sorted_rules.sort_by_key(|r| r.priority);

    for rule in &sorted_rules {
        let rule_expr = build_nft_rule(peer_ip, rule);
        if let Err(e) = run_nft(&["add", "rule", "inet", "wgwm", "forward", &rule_expr]) {
            tracing::error!("nftables: ルール追加失敗 (peer={}): {} — {}", peer_ip, rule_expr, e);
            return serde_json::json!({ "ok": false, "error": format!("ルール追加失敗: {}", e) });
        }
    }

    tracing::info!(
        "nftables: peer={} のACLを適用しました ({} ルール, {} 削除)",
        peer_ip, sorted_rules.len(), handles_to_delete.len()
    );
    serde_json::json!({ "ok": true })
}

/// 全ピアの ACL を一括再適用（起動時リストア）
fn reload_all_acl(peers: &[PeerAclEntry]) -> Value {
    // テーブルごと再作成して全ルールをクリーンに適用する
    if wgwm_table_exists() {
        if let Err(e) = run_nft(&["flush", "table", "inet", "wgwm"]) {
            tracing::warn!("nftables: wgwm テーブルの flush に失敗: {}", e);
        }
    }
    if let Err(e) = ensure_wgwm_table() {
        return serde_json::json!({ "ok": false, "error": e });
    }

    let mut errors: Vec<String> = Vec::new();
    for entry in peers {
        if entry.rules.is_empty() {
            continue;
        }
        let mut sorted = entry.rules.clone();
        sorted.sort_by_key(|r| r.priority);
        for rule in &sorted {
            let rule_expr = build_nft_rule(&entry.peer_ip, rule);
            if let Err(e) = run_nft(&["add", "rule", "inet", "wgwm", "forward", &rule_expr]) {
                errors.push(format!("peer={} rule={}: {}", entry.peer_ip, rule_expr, e));
            }
        }
    }

    if errors.is_empty() {
        tracing::info!("nftables: 全ACLを再適用しました ({} ピア)", peers.len());
        serde_json::json!({ "ok": true })
    } else {
        tracing::error!("nftables: 再適用中にエラーが発生しました: {:?}", errors);
        serde_json::json!({ "ok": false, "error": errors.join("; ") })
    }
}

fn handle_update_wireguard() -> Value {
    tracing::info!("wireguard-tools のアップデートを開始します");
    let update_out = Command::new("apt-get")
        .args(["-qq", "update"])
        .output();
    if let Err(e) = update_out {
        return serde_json::json!({ "ok": false, "error": format!("apt-get update 実行失敗: {}", e) });
    }
    match Command::new("apt-get").args(["install", "-y", "wireguard-tools"]).output() {
        Ok(out) if out.status.success() => {
            let output = String::from_utf8_lossy(&out.stdout).to_string();
            tracing::info!("wireguard-tools のアップデートが完了しました");
            serde_json::json!({ "ok": true, "output": output })
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let code = out.status.code().unwrap_or(-1);
            tracing::error!("wireguard-tools アップデート失敗: exit={} stderr={}", code, stderr);
            serde_json::json!({ "ok": false, "error": format!("apt-get install 失敗 (exit {}): {}", code, stderr) })
        }
        Err(e) => {
            tracing::error!("apt-get install 実行失敗: {}", e);
            serde_json::json!({ "ok": false, "error": format!("apt-get install 実行失敗: {}", e) })
        }
    }
}

fn handle_request(interface: &str, req: &Value) -> Value {
    let cmd = req.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
    tracing::debug!("リクエスト受信: cmd={}", cmd);
    match cmd {
        "get_public_key" => handle_get_public_key(interface),
        "get_peer_stats" => handle_get_peer_stats(interface),
        "peer_set" => {
            let pk = req.get("public_key").and_then(|v| v.as_str()).unwrap_or("");
            let allowed_ips: Vec<String> = req
                .get("allowed_ips")
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
        "update_wireguard" => handle_update_wireguard(),
        "apply_acl_rules" => {
            let peer_ip = req.get("peer_ip").and_then(|v| v.as_str()).unwrap_or("");
            let rules: Vec<AclRule> = req
                .get("rules")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            apply_peer_acl(peer_ip, &rules)
        }
        "reload_all_acl" => {
            let peers: Vec<PeerAclEntry> = req
                .get("peers")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            reload_all_acl(&peers)
        }
        _ => {
            tracing::warn!("不明なコマンドを受信しました: '{}'", cmd);
            serde_json::json!({ "ok": false, "error": format!("unknown cmd: {}", cmd) })
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    #[cfg(unix)]
    {
        if nix::unistd::geteuid().as_raw() != 0 {
            tracing::error!(
                "wg-worker は root 権限で起動する必要があります。\
                 systemd unit の User=root を確認してください"
            );
            std::process::exit(1);
        }
    }

    let settings = load_config().map_err(|e| {
        tracing::error!("設定ファイルの読み込みに失敗しました: {}", e);
        tracing::error!(
            "CONFIG_PATH 環境変数、または {} が正しいか確認してください \
             (journalctl -u wireguard-webmanager-worker で詳細を確認)",
            wg_common::config::DEFAULT_CONFIG_PATH
        );
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
        std::fs::create_dir_all(parent).map_err(|e| {
            let msg = format!(
                "ソケットディレクトリ '{}' の作成に失敗しました: {}",
                parent.display(),
                e
            );
            tracing::error!("{}", msg);
            msg
        })?;
    }
    if socket_path.exists() {
        std::fs::remove_file(socket_path).map_err(|e| {
            let msg = format!(
                "古いソケットファイル '{}' の削除に失敗しました: {}",
                socket_path.display(),
                e
            );
            tracing::error!("{}", msg);
            msg
        })?;
    }

    let listener = std::os::unix::net::UnixListener::bind(socket_path).map_err(|e| {
        let msg = format!(
            "UNIX ソケット '{}' のバインドに失敗しました: {}",
            socket_path.display(),
            e
        );
        tracing::error!("{}", msg);
        msg
    })?;

    #[cfg(unix)]
    {
        // chmod 660, chown を socket_owner に設定
        let meta = std::fs::metadata(socket_path)?;
        let mut perms = meta.permissions();
        perms.set_mode(0o660);
        std::fs::set_permissions(socket_path, perms)?;

        match User::from_name(socket_owner).map_err(|e| format!("ユーザー検索エラー: {e}"))? {
            Some(u) => {
                let uid = Uid::from_raw(u.uid.as_raw());
                let gid = Gid::from_raw(u.gid.as_raw());
                if let Err(e) = chown(socket_path, Some(uid), Some(gid)) {
                    tracing::warn!(
                        "chown({}) 失敗: {} - ソケットのオーナー変更をスキップします",
                        socket_owner,
                        e
                    );
                }
            }
            None => {
                tracing::warn!(
                    "ユーザー '{}' が見つかりません。ソケットのオーナーは変更されません。\
                     wg-manager が接続できない場合は useradd -r {} で作成してください",
                    socket_owner,
                    socket_owner
                );
            }
        }
    }

    tracing::info!(
        "wg-worker v{} 起動完了: socket={} interface={}",
        env!("CARGO_PKG_VERSION"),
        socket_path.display(),
        interface
    );

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("接続受付エラー: {}", e);
                continue;
            }
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
            tracing::debug!("空リクエストを受信しました");
            serde_json::json!({ "ok": false, "error": "empty request" })
        } else {
            let req: Value = serde_json::from_str(&line).unwrap_or_else(|e| {
                tracing::warn!("JSON パースエラー: {}", e);
                serde_json::json!({ "cmd": "" })
            });
            handle_request(interface, &req)
        };
        let out = serde_json::to_string(&response).unwrap_or_default() + "\n";
        let _ = stream.write_all(out.as_bytes());
    }
    Ok(())
}

use std::io::{Read, Write};
