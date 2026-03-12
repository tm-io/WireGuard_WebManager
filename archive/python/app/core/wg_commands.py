from __future__ import annotations

import re
import subprocess
from pathlib import Path
from typing import Iterable, List, Dict, Any, Tuple

from .config import Settings
from .exceptions import WireGuardCommandError


def _run_command(cmd: list[str]) -> str:
    """
    小さなヘルパー: コマンドを実行して標準出力を文字列で返す。
    エラー時には WireGuardCommandError を送出する。
    """
    try:
        completed = subprocess.run(
            cmd,
            check=True,
            text=True,
            capture_output=True,
        )
    except subprocess.CalledProcessError as exc:  # pragma: no cover - 実環境依存
        raise WireGuardCommandError(
            f"コマンド実行に失敗しました: {' '.join(cmd)}",
            returncode=exc.returncode,
            stderr=exc.stderr,
        ) from exc

    return completed.stdout.strip()


def generate_private_key() -> str:
    """
    `wg genkey` により秘密鍵を生成する。
    """
    return _run_command(["wg", "genkey"])


def generate_public_key(private_key: str) -> str:
    """
    `wg pubkey` に秘密鍵をパイプして公開鍵を生成する。
    """
    try:
        completed = subprocess.run(  # pragma: no cover - 実環境依存
            ["wg", "pubkey"],
            input=private_key,
            text=True,
            capture_output=True,
            check=True,
        )
    except subprocess.CalledProcessError as exc:
        raise WireGuardCommandError(
            "公開鍵の生成に失敗しました (wg pubkey)",
            returncode=exc.returncode,
            stderr=exc.stderr,
        ) from exc

    return completed.stdout.strip()


def generate_preshared_key() -> str:
    """
    `wg genpsk` により事前共有鍵を生成する。
    """
    return _run_command(["wg", "genpsk"])


def get_wg_version() -> str | None:
    """
    `wg --version` の出力からバージョン文字列を取得する。
    取得できない場合は None。root 不要。
    """
    try:
        out = subprocess.run(
            ["wg", "--version"],
            text=True,
            capture_output=True,
            timeout=5,
        )
        if out.returncode != 0:
            return None
        raw = (out.stdout or out.stderr or "").strip()
        # "wireguard-tools v1.0.20210914" や "1.0.20210914" 形式を想定
        m = re.search(r"v?(\d+\.\d+\.\d+)", raw)
        return m.group(1) if m else (raw or None)
    except (FileNotFoundError, subprocess.TimeoutExpired, Exception):
        return None


def _parse_wg_version(v: str) -> Tuple[int, ...]:
    """1.0.20210914 形式を (1, 0, 20210914) にパース。比較用。"""
    try:
        return tuple(int(x) for x in v.split(".")[:3])
    except (ValueError, AttributeError):
        return (0, 0, 0)


def get_server_public_key(settings: Settings) -> str:
    """
    稼働中の WireGuard インターフェースからサーバー公開鍵を取得する。
    wg_worker_socket が設定されていれば Worker 経由、否则は sudo wg を実行する。
    """
    if settings.paths.wg_worker_socket:
        from . import wg_worker_client
        return wg_worker_client.get_server_public_key(settings)
    interface = settings.wireguard.interface
    return _run_command(["sudo", "wg", "show", interface, "public-key"])


def get_interface_peer_stats(settings: Settings) -> List[Dict[str, Any]]:
    """
    Peer ごとのステータスを取得する。
    wg_worker_socket が設定されていれば Worker 経由、否则は sudo wg show dump。
    """
    if settings.paths.wg_worker_socket:
        from . import wg_worker_client
        return wg_worker_client.get_interface_peer_stats(settings)
    interface = settings.wireguard.interface
    out = _run_command(["sudo", "wg", "show", interface, "dump"])
    lines = out.splitlines()
    if not lines:
        return []

    peers: List[Dict[str, Any]] = []
    for line in lines[1:]:
        cols = line.split("\t")
        if len(cols) < 8:
            continue
        peers.append(
            {
                "public_key": cols[0],
                "endpoint": cols[2] or None,
                "allowed_ips": [ip for ip in cols[3].split(",") if ip] if cols[3] else [],
                "latest_handshake": int(cols[4]) if cols[4] and cols[4] != "0" else None,
                "rx_bytes": int(cols[5]) if cols[5] else 0,
                "tx_bytes": int(cols[6]) if cols[6] else 0,
            }
        )
    return peers


def apply_config_with_syncconf(settings: Settings, tmp_config_path: Path) -> None:
    """
    生成済みの設定ファイルを `wg syncconf` で動的に反映する。

    Phase 2 では「設定ファイルをどこでどう作るか」はまだ決めないため、
    呼び出し元から一時ファイルパスを受け取るインターフェースにしている。
    """
    interface = settings.wireguard.interface
    conf_path = str(tmp_config_path)
    _run_command(["wg", "syncconf", interface, conf_path])


def apply_peer_changes_with_set(
    settings: Settings,
    *,
    public_key: str,
    allowed_ips: Iterable[str],
    preshared_key: str | None = None,
    remove: bool = False,
) -> None:
    """
    特定 Peer の追加・更新・削除を反映する。
    wg_worker_socket が設定されていれば Worker 経由、否则は sudo wg set。
    """
    if settings.paths.wg_worker_socket:
        from . import wg_worker_client
        if remove:
            wg_worker_client.peer_remove(settings, public_key=public_key)
        else:
            wg_worker_client.peer_set(
                settings,
                public_key=public_key,
                allowed_ips=list(allowed_ips),
                preshared_key=preshared_key,
            )
        return
    interface = settings.wireguard.interface
    base_cmd = ["sudo", "wg", "set", interface, "peer", public_key]
    if remove:
        _run_command(base_cmd + ["remove"])
        return
    cmd: list[str] = base_cmd + ["allowed-ips", ",".join(allowed_ips)]
    if preshared_key:
        cmd.extend(["preshared-key", preshared_key])
    _run_command(cmd)

