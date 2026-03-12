//! Worker を使わない場合のローカル wg 実行（sudo wg）

use std::process::Command;

pub fn get_wg_version() -> Option<String> {
    let out = Command::new("wg").arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

pub fn sudo_wg_dump(interface: &str) -> Result<String, String> {
    sudo_wg(&["show", interface, "dump"])
}

pub fn sudo_wg(args: &[&str]) -> Result<String, String> {
    let out = Command::new("sudo")
        .arg("wg")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("exit {}", out.status.code().unwrap_or(-1))
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn sudo_wg_set_peer(
    interface: &str,
    public_key: &str,
    allowed_ips: &str,
    preshared_key: Option<&str>,
) -> Result<(), String> {
    // wg set の preshared-key はファイルパスを要求するので、一時ファイルに書く
    let mut args: Vec<String> = vec![
        "set".into(),
        interface.into(),
        "peer".into(),
        public_key.into(),
        "allowed-ips".into(),
        allowed_ips.into(),
    ];
    let mut psk_path: Option<std::path::PathBuf> = None;
    if let Some(psk) = preshared_key {
        let mut tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        use std::io::Write;
        tmp.write_all(psk.as_bytes()).map_err(|e| e.to_string())?;
        let (_, path) = tmp.keep().map_err(|e| e.to_string())?;
        args.push("preshared-key".into());
        args.push(path.to_string_lossy().into_owned());
        psk_path = Some(path);
    }

    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = sudo_wg(&args_ref).map(|_| ());
    if let Some(p) = psk_path.as_ref() {
        let _ = std::fs::remove_file(p);
    }
    r
}

pub fn generate_private_key() -> Result<String, String> {
    let out = Command::new("wg").arg("genkey").output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn generate_public_key(private_key: &str) -> Result<String, String> {
    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(private_key.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn generate_preshared_key() -> Result<String, String> {
    let out = Command::new("wg").arg("genpsk").output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

