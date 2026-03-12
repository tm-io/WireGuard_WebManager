#!/usr/bin/env python3
"""
WireGuard 特権操作用 Worker（root で systemd から起動）。

- UNIX ドメインソケットでリクエストを受け付け、許可された wg 操作のみ実行する。
- ソケットの所有者を config の socket_owner に設定し、Webアプリユーザーのみ接続可能にする。
- sudo 不要で root 権限で wg を実行するため、sudoers 設定が不要になる。

起動例（root）:
  CONFIG_PATH=/path/to/config.yaml python3 wg_worker.py
"""
from __future__ import annotations

import json
import os
import pwd
import socket
import subprocess
import sys
import tempfile
from pathlib import Path

# プロジェクト直下の config を読むため
SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

try:
    import yaml
except ImportError:
    yaml = None


def load_config() -> dict:
    config_path = os.environ.get("CONFIG_PATH")
    if config_path and Path(config_path).is_file():
        path = Path(config_path)
    else:
        path = SCRIPT_DIR / "config.yaml"
    if not path.is_file():
        raise SystemExit(f"Config not found: {path}")

    if yaml is None:
        raise SystemExit("PyYAML required. pip install pyyaml")

    with path.open("r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def run_wg(args: list[str], input_text: str | None = None) -> tuple[str, str, int]:
    """wg を subprocess で実行（root なので sudo 不要）。"""
    try:
        r = subprocess.run(
            ["wg"] + args,
            input=input_text,
            text=True,
            capture_output=True,
            timeout=30,
        )
        return r.stdout.strip(), r.stderr.strip(), r.returncode
    except subprocess.TimeoutExpired:
        return "", "timeout", -1
    except Exception as e:
        return "", str(e), -1


def handle_get_public_key(interface: str) -> dict:
    out, err, code = run_wg(["show", interface, "public-key"])
    if code != 0:
        return {"ok": False, "error": err or out or f"exit {code}"}
    return {"ok": True, "public_key": out}


def handle_get_peer_stats(interface: str) -> dict:
    out, err, code = run_wg(["show", interface, "dump"])
    if code != 0:
        return {"ok": False, "error": err or out or f"exit {code}"}
    lines = out.splitlines()
    if not lines:
        return {"ok": True, "peers": []}
    peers = []
    for line in lines[1:]:
        cols = line.split("\t")
        if len(cols) < 8:
            continue
        peers.append({
            "public_key": cols[0],
            "endpoint": cols[2] or None,
            "allowed_ips": [x for x in cols[3].split(",") if x] if cols[3] else [],
            "latest_handshake": int(cols[4]) if cols[4] and cols[4] != "0" else None,
            "rx_bytes": int(cols[5]) if cols[5] else 0,
            "tx_bytes": int(cols[6]) if cols[6] else 0,
        })
    return {"ok": True, "peers": peers}


def handle_peer_set(interface: str, public_key: str, allowed_ips: list, preshared_key: str | None) -> dict:
    if not public_key or not allowed_ips:
        return {"ok": False, "error": "public_key and allowed_ips required"}
    cmd = ["set", interface, "peer", public_key, "allowed-ips", ",".join(allowed_ips)]
    tmp_psk_path = None
    if preshared_key:
        # wg set の preshared-key は「ファイルパス」を取る。鍵を一時ファイルに書き、パスを渡す。
        try:
            fd, tmp_psk_path = tempfile.mkstemp(prefix="wg-psk-", text=True)
            os.write(fd, preshared_key.encode("utf-8"))
            os.close(fd)
            os.chmod(tmp_psk_path, 0o600)
            cmd.extend(["preshared-key", tmp_psk_path])
        except OSError as e:
            if tmp_psk_path and os.path.exists(tmp_psk_path):
                try:
                    os.unlink(tmp_psk_path)
                except OSError:
                    pass
            return {"ok": False, "error": f"preshared-key temp file: {e}"}
    try:
        _, err, code = run_wg(cmd)
        if code != 0:
            emsg = err or f"exit {code}"
            print(f"wg-manager-worker: peer_set failed interface={interface} public_key={public_key[:16]}...: {emsg}", file=sys.stderr)
            return {"ok": False, "error": emsg}
        return {"ok": True}
    finally:
        if tmp_psk_path and os.path.exists(tmp_psk_path):
            try:
                os.unlink(tmp_psk_path)
            except OSError:
                pass


def handle_peer_remove(interface: str, public_key: str) -> dict:
    if not public_key:
        return {"ok": False, "error": "public_key required"}
    _, err, code = run_wg(["set", interface, "peer", public_key, "remove"])
    if code != 0:
        emsg = err or f"exit {code}"
        print(f"wg-manager-worker: peer_remove failed interface={interface} public_key={public_key[:16]}...: {emsg}", file=sys.stderr)
        return {"ok": False, "error": emsg}
    return {"ok": True}


def handle_request(interface: str, req: dict) -> dict:
    cmd = req.get("cmd")
    if cmd == "get_public_key":
        return handle_get_public_key(interface)
    if cmd == "get_peer_stats":
        return handle_get_peer_stats(interface)
    if cmd == "peer_set":
        return handle_peer_set(
            interface,
            req.get("public_key", ""),
            req.get("allowed_ips") or [],
            req.get("preshared_key"),
        )
    if cmd == "peer_remove":
        return handle_peer_remove(interface, req.get("public_key", ""))
    return {"ok": False, "error": f"unknown cmd: {cmd}"}


def main() -> None:
    if os.geteuid() != 0:
        print("wg_worker must be run as root.", file=sys.stderr)
        sys.exit(1)

    cfg = load_config()
    paths = cfg.get("paths", {})
    wg = cfg.get("wireguard", {})
    socket_path = paths.get("wg_worker_socket") or "/var/run/wg-manager.sock"
    socket_owner = paths.get("socket_owner") or "kanri"
    interface = wg.get("interface") or "wg0"

    # ソケットの親ディレクトリが無ければ作成（例: /var/run/wg-manager/ など）
    Path(socket_path).parent.mkdir(parents=True, exist_ok=True)

    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    if Path(socket_path).exists():
        Path(socket_path).unlink()
    sock.bind(socket_path)
    sock.listen(32)

    try:
        uid = pwd.getpwnam(socket_owner).pw_uid
        gid = pwd.getpwnam(socket_owner).pw_gid
        os.chown(socket_path, uid, gid)
        os.chmod(socket_path, 0o660)
    except KeyError:
        print(f"User {socket_owner!r} not found; socket ownership not changed.", file=sys.stderr)

    print(f"Listening on {socket_path} (interface={interface})", file=sys.stderr)

    while True:
        conn, _ = sock.accept()
        try:
            buf = b""
            while True:
                chunk = conn.recv(4096)
                if not chunk:
                    break
                buf += chunk
                if b"\n" in buf:
                    break
            line = buf.decode("utf-8", errors="replace").strip()
            if not line:
                conn.sendall(b'{"ok":false,"error":"empty request"}\n')
                continue
            req = json.loads(line)
            resp = handle_request(interface, req)
            conn.sendall((json.dumps(resp, ensure_ascii=False) + "\n").encode("utf-8"))
        except (json.JSONDecodeError, BrokenPipeError, ConnectionResetError) as e:
            try:
                conn.sendall(json.dumps({"ok": False, "error": str(e)}, ensure_ascii=False).encode("utf-8") + b"\n")
            except Exception:
                pass
        finally:
            conn.close()


if __name__ == "__main__":
    main()
