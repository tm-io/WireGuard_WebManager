from __future__ import annotations

import json
import urllib.request
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Request
from starlette.concurrency import run_in_threadpool

from app.core.config import Settings
from app.core.db import Database
from app.core.wg_commands import (
    WireGuardCommandError,
    get_server_public_key,
    get_interface_peer_stats,
    get_wg_version,
    _parse_wg_version,
)
from app.repositories import peers as peers_repo


router = APIRouter(prefix="/api/server", tags=["server"])


def get_settings(request: Request) -> Settings:
    return request.app.state.settings


def get_db(request: Request) -> Database:
    return request.app.state.db


def _wg_error_to_http(exc: WireGuardCommandError, default_message: str):
    """Worker 未起動時は 503 と案内を返す。それ以外は 500。"""
    msg = str(exc)
    if "Worker に接続できません" in msg:
        return HTTPException(
            status_code=503,
            detail="Worker が起動していません。先に Worker を起動してください: sudo systemctl start wg-manager-worker",
        )
    return HTTPException(status_code=500, detail=f"{default_message}: {exc}")


@router.get("/public-key")
async def read_server_public_key(settings: Settings = Depends(get_settings)):
    """
    稼働中の WireGuard インターフェースから公開鍵を取得して返す。
    - 内部的には Worker 経由または `wg show <interface> public-key` を実行する。
    """
    try:
        pub = get_server_public_key(settings)
    except WireGuardCommandError as exc:
        raise _wg_error_to_http(exc, "wg から公開鍵を取得できませんでした") from exc

    return {"public_key": pub}


@router.get("/peer-stats")
async def read_peer_stats(
    settings: Settings = Depends(get_settings),
    db: Database = Depends(get_db),
):
    """
    WireGuard の実ステータスと DB の peers を突き合わせ、
    各 Peer の接続状態とトラフィック量を返す。
    """
    db_peers = await run_in_threadpool(peers_repo.list_peers, db)
    try:
        wg_peers = get_interface_peer_stats(settings)
    except WireGuardCommandError as exc:
        raise _wg_error_to_http(exc, "wg からステータスを取得できませんでした") from exc

    wg_by_pub = {p["public_key"]: p for p in wg_peers}

    result = []
    for peer in db_peers:
        s = wg_by_pub.get(peer.public_key)
        if s and s.get("latest_handshake"):
            connected = True
        else:
            connected = False

        result.append(
            {
                "id": peer.id,
                "public_key": peer.public_key,
                "connected": connected,
                "latest_handshake": s.get("latest_handshake") if s else None,
                "rx_bytes": s.get("rx_bytes") if s else 0,
                "tx_bytes": s.get("tx_bytes") if s else 0,
            }
        )

    return {"peers": result}


def _fetch_latest_wg_version() -> str | None:
    """GitHub API から wireguard-tools の最新リリースタグを取得する。"""
    try:
        req = urllib.request.Request(
            "https://api.github.com/repos/WireGuard/wireguard-tools/releases/latest",
            headers={"Accept": "application/vnd.github.v3+json"},
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            data = json.loads(resp.read().decode())
        tag = data.get("tag_name") or ""
        if tag.startswith("v"):
            tag = tag[1:]
        return tag if tag else None
    except Exception:
        return None


@router.get("/wg-version")
async def read_wg_version():
    """
    インストール中の WireGuard (wireguard-tools) のバージョンと、
    取得できれば最新版を返す。古い場合は outdated が true。
    """
    current_raw: str | None = None
    try:
        current_raw = await run_in_threadpool(get_wg_version)
    except Exception:
        pass
    current = current_raw or None
    latest: str | None = await run_in_threadpool(_fetch_latest_wg_version)
    outdated = False
    if current and latest:
        try:
            outdated = _parse_wg_version(current) < _parse_wg_version(latest)
        except Exception:
            pass
    return {
        "current": current,
        "current_raw": current_raw,
        "latest": latest,
        "outdated": outdated,
    }

