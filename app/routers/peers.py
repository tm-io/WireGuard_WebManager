from __future__ import annotations

import functools
import logging
from typing import List, Optional

logger = logging.getLogger("app.peers")

from fastapi import APIRouter, Depends, HTTPException, Request, status
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from app.core.config import Settings
from app.core.db import Database
from app.core.ip_allocator import IPAllocationError, allocate_next_ip
from app.core.wg_commands import (
    WireGuardCommandError,
    generate_preshared_key,
    generate_private_key,
    generate_public_key,
    apply_peer_changes_with_set,
)
from app.core.conf_templates import build_client_conf_text
from app.repositories import peers as peers_repo
import io
import re
from urllib.parse import quote
import qrcode
from fastapi.responses import Response, StreamingResponse


router = APIRouter(prefix="/api/peers", tags=["peers"])


def get_settings(request: Request) -> Settings:
    return request.app.state.settings


def get_db(request: Request) -> Database:
    return request.app.state.db


class PeerCreateRequest(BaseModel):
    name: str
    is_active: bool = True


class PeerUpdateRequest(BaseModel):
    name: Optional[str] = None
    is_active: Optional[bool] = None


class PeerResponse(BaseModel):
    id: int
    name: str
    public_key: str
    allocated_ip: str
    is_active: bool
    created_at: str

    @classmethod
    def from_peer(cls, peer: peers_repo.Peer) -> "PeerResponse":
        return cls(
            id=peer.id,
            name=peer.name,
            public_key=peer.public_key,
            allocated_ip=peer.allocated_ip,
            is_active=peer.is_active,
            created_at=peer.created_at,
        )


class PeerConfResponse(BaseModel):
    id: int
    name: str
    config_text: str


@router.get("/", response_model=List[PeerResponse])
async def list_peers(
    db: Database = Depends(get_db),
) -> List[PeerResponse]:
    peers = await run_in_threadpool(peers_repo.list_peers, db)
    return [PeerResponse.from_peer(p) for p in peers]


@router.post("/", response_model=PeerResponse, status_code=status.HTTP_201_CREATED)
async def create_peer(
    payload: PeerCreateRequest,
    settings: Settings = Depends(get_settings),
    db: Database = Depends(get_db),
) -> PeerResponse:
    try:
        used_ips = await run_in_threadpool(peers_repo.list_allocated_ips, db)
        next_ip = allocate_next_ip(settings.wireguard.client_ip_range, used_ips)
    except IPAllocationError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    if not next_ip:
        raise HTTPException(
            status_code=400,
            detail="利用可能なIPアドレスが残っていません。",
        )

    try:
        private_key = generate_private_key()
        public_key = generate_public_key(private_key)
        psk = generate_preshared_key()
    except WireGuardCommandError as exc:
        raise HTTPException(
            status_code=500,
            detail=f"WireGuard鍵生成に失敗しました: {exc}",
        ) from exc

    peer = await run_in_threadpool(
        peers_repo.create_peer,
        db,
        name=payload.name,
        public_key=public_key,
        private_key_encrypted=private_key,  # 将来的に暗号化を導入
        pre_shared_key=psk,
        allocated_ip=next_ip,
        is_active=payload.is_active,
    )

    # Phase 3 では DB 登録まで。WireGuard への即時反映は Phase 2 のAPI化時に追加。

    return PeerResponse.from_peer(peer)


@router.get("/{peer_id}", response_model=PeerResponse)
async def get_peer(
    peer_id: int,
    db: Database = Depends(get_db),
) -> PeerResponse:
    peer = await run_in_threadpool(peers_repo.get_peer, db, peer_id)
    if not peer:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")
    return PeerResponse.from_peer(peer)


@router.put("/{peer_id}", response_model=PeerResponse)
async def update_peer(
    peer_id: int,
    payload: PeerUpdateRequest,
    settings: Settings = Depends(get_settings),
    db: Database = Depends(get_db),
) -> PeerResponse:
    peer = await run_in_threadpool(peers_repo.get_peer, db, peer_id)
    if not peer:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")

    # is_active 変更時は「先に WireGuard に反映し、成功したら DB を更新」する（DB と wg の整合を保つ）
    if payload.is_active is not None:
        try:
            if payload.is_active:
                fn = functools.partial(
                    apply_peer_changes_with_set,
                    settings,
                    public_key=peer.public_key,
                    allowed_ips=[f"{peer.allocated_ip}/32"],
                    preshared_key=peer.pre_shared_key,
                    remove=False,
                )
            else:
                fn = functools.partial(
                    apply_peer_changes_with_set,
                    settings,
                    public_key=peer.public_key,
                    allowed_ips=[f"{peer.allocated_ip}/32"],
                    preshared_key=peer.pre_shared_key,
                    remove=True,
                )
            await run_in_threadpool(fn)
        except WireGuardCommandError as exc:
            msg = str(exc)
            err_detail = msg
            if getattr(exc, "stderr", None) and str(exc.stderr).strip():
                err_detail += " stderr=%s" % (exc.stderr.strip(),)
            if getattr(exc, "response", None):
                err_detail += " response=%s" % (exc.response,)
            logger.error(
                "Peer有効/無効の切り替え失敗(peer_id=%s, is_active=%s): %s",
                peer_id,
                payload.is_active,
                err_detail,
                exc_info=True,
            )
            if "Worker に接続できません" in msg:
                raise HTTPException(
                    status_code=503,
                    detail="Worker が起動していません。先に Worker を起動してください: sudo systemctl start wg-manager-worker",
                ) from exc
            detail = f"WireGuard設定の更新に失敗しました: {exc}"
            if getattr(exc, "stderr", None) and str(exc.stderr).strip():
                detail += f" 詳細: {exc.stderr.strip()}"
            raise HTTPException(status_code=500, detail=detail) from exc
        except Exception as exc:
            logger.error(
                "Peer有効/無効の切り替え中に予期しないエラー(peer_id=%s): %s",
                peer_id,
                exc,
                exc_info=True,
            )
            raise HTTPException(
                status_code=500,
                detail=f"状態の反映中にエラーが発生しました: {exc!s}",
            ) from exc

    peer = await run_in_threadpool(
        peers_repo.update_peer,
        db,
        peer_id,
        name=payload.name,
        is_active=payload.is_active,
    )
    if not peer:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")
    return PeerResponse.from_peer(peer)


@router.delete("/{peer_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_peer(
    peer_id: int,
    db: Database = Depends(get_db),
) -> None:
    ok = await run_in_threadpool(peers_repo.delete_peer, db, peer_id)
    if not ok:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")


@router.get("/{peer_id}/conf", response_model=PeerConfResponse)
async def get_peer_conf(
    peer_id: int,
    server_public_key: str,
    settings: Settings = Depends(get_settings),
    db: Database = Depends(get_db),
) -> PeerConfResponse:
    """
    指定 Peer のクライアント用 .conf テキストを返す。

    - サーバー公開鍵は現時点では config/DB に持たないため、クエリパラメータとして渡す。
      例: `/api/peers/1/conf?server_public_key=XXXXX`
    """
    peer = await run_in_threadpool(peers_repo.get_peer, db, peer_id)
    if not peer:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")

    text = build_client_conf_text(
        settings,
        peer,
        server_public_key=server_public_key,
    )
    return PeerConfResponse(id=peer.id, name=peer.name, config_text=text)


def _safe_conf_filename(name: str, peer_id: int) -> str:
    """ファイル名に使えるようにピア名をサニタイズし、.conf を付ける。"""
    safe = re.sub(r"[^\w\u3040-\u309f\u30a0-\u30ff\u4e00-\u9fff\-\.]", "-", name or "")
    safe = re.sub(r"-+", "-", safe).strip("-") or f"peer-{peer_id}"
    return f"{safe}.conf"


@router.get("/{peer_id}/conf/download")
async def download_peer_conf(
    peer_id: int,
    server_public_key: str,
    settings: Settings = Depends(get_settings),
    db: Database = Depends(get_db),
):
    """
    指定 Peer のクライアント用 .conf をファイルとして返す（Windows/Mac/Linux 用）。
    Content-Disposition でダウンロードファイル名を付与する。
    """
    peer = await run_in_threadpool(peers_repo.get_peer, db, peer_id)
    if not peer:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")

    text = build_client_conf_text(
        settings,
        peer,
        server_public_key=server_public_key,
    )
    filename = _safe_conf_filename(peer.name, peer.id)
    return Response(
        content=text.encode("utf-8"),
        media_type="text/plain; charset=utf-8",
        headers={
            "Content-Disposition": f'attachment; filename="{filename}"; filename*=UTF-8\'\'{quote(filename)}',
        },
    )


@router.get("/{peer_id}/qr")
async def get_peer_conf_qr(
    peer_id: int,
    server_public_key: str,
    settings: Settings = Depends(get_settings),
    db: Database = Depends(get_db),
):
    """
    指定 Peer のクライアント用 .conf を QR コード(PNG)として返す。
    """
    peer = await run_in_threadpool(peers_repo.get_peer, db, peer_id)
    if not peer:
        raise HTTPException(status_code=404, detail="Peerが見つかりません。")

    text = build_client_conf_text(
        settings,
        peer,
        server_public_key=server_public_key,
    )
    img = qrcode.make(text)
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    buf.seek(0)
    return StreamingResponse(buf, media_type="image/png")

