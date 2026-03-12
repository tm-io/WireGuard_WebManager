from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict

import yaml

from .path_utils import get_resource_path


@dataclass
class AppConfig:
    host: str
    port: int
    auth_username: str
    auth_password: str


@dataclass
class PathsConfig:
    db_path: str
    wg_conf_dir: str
    wg_worker_socket: str  # 空でなければ Worker 経由で wg 実行（sudo 不要）
    socket_owner: str  # Worker がソケットを chown するユーザー名


@dataclass
class WireGuardConfig:
    interface: str
    server_endpoint: str
    listen_port: int
    client_ip_range: str
    client_dns: str
    persistent_keepalive: int


@dataclass
class Settings:
    app: AppConfig
    paths: PathsConfig
    wireguard: WireGuardConfig


def _load_yaml(path: Path) -> Dict[str, Any]:
    with path.open("r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def get_config_path() -> Path:
    """config.yaml のパスを返す（保存時に使用）。"""
    return get_resource_path("config.yaml")


def load_settings(config_path: Path | None = None) -> Settings:
    """
    config.yaml を読み込み Settings オブジェクトを返す。
    明示パスが渡されなければ、プロジェクトルート直下の config.yaml を探す。
    """
    if config_path is None:
        config_path = get_config_path()

    if not config_path.is_file():
        raise FileNotFoundError(f"config.yaml が見つかりません: {config_path}")

    raw = _load_yaml(config_path)

    app_cfg = raw.get("app", {})
    paths_cfg = raw.get("paths", {})
    wg_cfg = raw.get("wireguard", {})

    app = AppConfig(
        host=str(app_cfg.get("host", "0.0.0.0")),
        port=int(app_cfg.get("port", 8080)),
        auth_username=str(app_cfg.get("auth_username", "admin")),
        auth_password=str(app_cfg.get("auth_password", "password123")),
    )

    paths = PathsConfig(
        db_path=str(paths_cfg.get("db_path", "data/wg-manager.db")),
        wg_conf_dir=str(paths_cfg.get("wg_conf_dir", "/etc/wireguard")),
        wg_worker_socket=str(paths_cfg.get("wg_worker_socket", "")),
        socket_owner=str(paths_cfg.get("socket_owner", "kanri")),
    )

    wg = WireGuardConfig(
        interface=str(wg_cfg.get("interface", "wg0")),
        server_endpoint=str(wg_cfg.get("server_endpoint", "127.0.0.1")),
        listen_port=int(wg_cfg.get("listen_port", 51820)),
        client_ip_range=str(wg_cfg.get("client_ip_range", "10.8.0.0/24")),
        client_dns=str(wg_cfg.get("client_dns", "1.1.1.1, 8.8.8.8")),
        persistent_keepalive=int(wg_cfg.get("persistent_keepalive", 25)),
    )

    return Settings(app=app, paths=paths, wireguard=wg)


def save_settings(raw: Dict[str, Any], config_path: Path | None = None) -> None:
    """
    raw 辞書を config.yaml に書き込む。
    既存ファイルがある場合は上書き。パス未指定なら get_config_path() を使用。
    """
    if config_path is None:
        config_path = get_config_path()
    with config_path.open("w", encoding="utf-8") as f:
        yaml.safe_dump(raw, f, allow_unicode=True, default_flow_style=False, sort_keys=False)
