"""
Web アプリから Worker（root）へ UNIX ソケット経由でリクエストを送るクライアント。

config.paths.wg_worker_socket が設定されている場合のみ使用する。
"""
from __future__ import annotations

import json
import socket
from typing import Any, Dict, List

from .config import Settings
from .exceptions import WireGuardCommandError


def _request(settings: Settings, payload: dict) -> dict:
    path = settings.paths.wg_worker_socket
    if not path:
        raise WireGuardCommandError("wg_worker_socket が設定されていません")

    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.settimeout(15)
    try:
        sock.connect(path)
        msg = (json.dumps(payload, ensure_ascii=False) + "\n").encode("utf-8")
        sock.sendall(msg)
        buf = b""
        while b"\n" not in buf:
            chunk = sock.recv(4096)
            if not chunk:
                break
            buf += chunk
        sock.close()
    except (FileNotFoundError, ConnectionRefusedError, socket.error) as e:
        raise WireGuardCommandError(
            f"Worker に接続できません（{path}）: {e}",
            response=None,
        ) from e
    finally:
        try:
            sock.close()
        except Exception:
            pass

    line = buf.decode("utf-8", errors="replace").strip()
    if not line:
        raise WireGuardCommandError("Worker から空の応答を受け取りました", response=None)
    try:
        out = json.loads(line)
    except json.JSONDecodeError as e:
        raise WireGuardCommandError(f"Worker の応答が不正です: {e}", response=None) from e

    if not out.get("ok"):
        err = out.get("error", "unknown error")
        raise WireGuardCommandError(f"Worker エラー: {err}", response=out)
    return out


def get_server_public_key(settings: Settings) -> str:
    out = _request(settings, {"cmd": "get_public_key"})
    return out["public_key"]


def get_interface_peer_stats(settings: Settings) -> List[Dict[str, Any]]:
    out = _request(settings, {"cmd": "get_peer_stats"})
    return out.get("peers", [])


def peer_set(
    settings: Settings,
    *,
    public_key: str,
    allowed_ips: List[str],
    preshared_key: str | None = None,
) -> None:
    payload: dict = {
        "cmd": "peer_set",
        "public_key": public_key,
        "allowed_ips": allowed_ips,
    }
    if preshared_key:
        payload["preshared_key"] = preshared_key
    _request(settings, payload)


def peer_remove(settings: Settings, *, public_key: str) -> None:
    _request(settings, {"cmd": "peer_remove", "public_key": public_key})
