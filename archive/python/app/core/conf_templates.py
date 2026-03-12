from __future__ import annotations

from app.core.config import Settings
from app.repositories.peers import Peer


def build_client_conf_text(
    settings: Settings,
    peer: Peer,
    *,
    server_public_key: str,
) -> str:
    """
    クライアント用 WireGuard .conf テキストを生成する。

    - サーバーの公開鍵は現時点ではDBに持たないため、引数で受け取る。
    """
    endpoint = f"{settings.wireguard.server_endpoint}:{settings.wireguard.listen_port}"
    dns = settings.wireguard.client_dns
    keepalive = settings.wireguard.persistent_keepalive

    lines = [
        "[Interface]",
        f"PrivateKey = {peer.private_key_encrypted}",
        f"Address = {peer.allocated_ip}/32",
        f"DNS = {dns}",
        "",
        "[Peer]",
        f"PublicKey = {server_public_key}",
        f"Endpoint = {endpoint}",
        "AllowedIPs = 0.0.0.0/0, ::/0",
        f"PersistentKeepalive = {keepalive}",
    ]

    if peer.pre_shared_key:
        lines.insert(
            len(lines) - 2,
            f"PresharedKey = {peer.pre_shared_key}",
        )

    return "\n".join(lines) + "\n"

