"""
設定（config.yaml）の取得・保存 API。認証済みユーザーのみ。
"""
from __future__ import annotations

import yaml
from pathlib import Path

from fastapi import APIRouter, Body, HTTPException, Request

from app.core.config import get_config_path, load_settings, save_settings
from app.core.path_utils import get_resource_path
from app.routers import auth as auth_router

router = APIRouter(prefix="/api/settings", tags=["settings"])


def _require_auth(request: Request) -> None:
    if request.cookies.get(auth_router.SESSION_COOKIE_NAME) != "authenticated":
        raise HTTPException(status_code=401, detail="ログインしてください。")


def _raw_from_file() -> dict:
    path = get_config_path()
    if not path.is_file():
        return {}
    with path.open("r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


@router.get("")
async def get_settings(request: Request) -> dict:
    """現在の設定を返す。auth_password は空で返す（UI で変更時のみ送る想定）。"""
    _require_auth(request)
    s = load_settings()
    raw = {
        "app": {
            "host": s.app.host,
            "port": s.app.port,
            "auth_username": s.app.auth_username,
            "auth_password": "",  # 画面には出さない
        },
        "paths": {
            "db_path": s.paths.db_path,
            "wg_conf_dir": s.paths.wg_conf_dir,
            "wg_worker_socket": s.paths.wg_worker_socket,
            "socket_owner": s.paths.socket_owner,
        },
        "wireguard": {
            "interface": s.wireguard.interface,
            "server_endpoint": s.wireguard.server_endpoint,
            "listen_port": s.wireguard.listen_port,
            "client_ip_range": s.wireguard.client_ip_range,
            "client_dns": s.wireguard.client_dns,
            "persistent_keepalive": s.wireguard.persistent_keepalive,
        },
    }
    return raw


@router.put("")
async def put_settings(request: Request, body: dict = Body(...)) -> dict:
    """設定を保存する。auth_password が空の場合は既存を維持。"""
    _require_auth(request)
    app_cfg = body.get("app") or {}
    paths_cfg = body.get("paths") or {}
    wg_cfg = body.get("wireguard") or {}

    current = _raw_from_file()
    cur_app = current.get("app") or {}
    # パスワード: 送信が空なら既存を維持、なければデフォルト
    new_password = (app_cfg.get("auth_password") or "").strip()
    auth_password = new_password if new_password else (cur_app.get("auth_password") or "password123")

    raw = {
        "app": {
            "host": str(app_cfg.get("host", cur_app.get("host", "0.0.0.0"))),
            "port": int(app_cfg.get("port", cur_app.get("port", 8080))),
            "auth_username": str(app_cfg.get("auth_username", cur_app.get("auth_username", "admin"))),
            "auth_password": auth_password,
        },
        "paths": {
            "db_path": str(paths_cfg.get("db_path", (current.get("paths") or {}).get("db_path", "data/wg-manager.db"))),
            "wg_conf_dir": str(paths_cfg.get("wg_conf_dir", (current.get("paths") or {}).get("wg_conf_dir", "/etc/wireguard"))),
            "wg_worker_socket": str(paths_cfg.get("wg_worker_socket", (current.get("paths") or {}).get("wg_worker_socket", ""))),
            "socket_owner": str(paths_cfg.get("socket_owner", (current.get("paths") or {}).get("socket_owner", "kanri"))),
        },
        "wireguard": {
            "interface": str(wg_cfg.get("interface", (current.get("wireguard") or {}).get("interface", "wg0"))),
            "server_endpoint": str(wg_cfg.get("server_endpoint", (current.get("wireguard") or {}).get("server_endpoint", "203.0.113.1"))),
            "listen_port": int(wg_cfg.get("listen_port", (current.get("wireguard") or {}).get("listen_port", 51820))),
            "client_ip_range": str(wg_cfg.get("client_ip_range", (current.get("wireguard") or {}).get("client_ip_range", "10.8.0.0/24"))),
            "client_dns": str(wg_cfg.get("client_dns", (current.get("wireguard") or {}).get("client_dns", "1.1.1.1, 8.8.8.8"))),
            "persistent_keepalive": int(wg_cfg.get("persistent_keepalive", (current.get("wireguard") or {}).get("persistent_keepalive", 25))),
        },
    }
    try:
        save_settings(raw)
    except OSError as e:
        raise HTTPException(status_code=500, detail=f"設定の保存に失敗しました: {e}")
    # メモリ上の設定を更新（host/port は再起動まで効かない）
    request.app.state.settings = load_settings()
    return {"ok": True, "message": "保存しました。一部の項目は再起動後に反映されます。"}