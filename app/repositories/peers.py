from __future__ import annotations

import datetime as dt
from dataclasses import dataclass
from typing import Iterable, List, Optional

from app.core.db import Database


@dataclass
class Peer:
    id: int
    name: str
    public_key: str
    private_key_encrypted: str
    pre_shared_key: Optional[str]
    allocated_ip: str
    is_active: bool
    created_at: str


def _row_to_peer(row) -> Peer:
    return Peer(
        id=row["id"],
        name=row["name"],
        public_key=row["public_key"],
        private_key_encrypted=row["private_key_encrypted"],
        pre_shared_key=row["pre_shared_key"],
        allocated_ip=row["allocated_ip"],
        is_active=bool(row["is_active"]),
        created_at=row["created_at"],
    )


def list_peers(db: Database) -> List[Peer]:
    with db.connect() as conn:
        cur = conn.execute(
            "SELECT id, name, public_key, private_key_encrypted, pre_shared_key, "
            "allocated_ip, is_active, created_at "
            "FROM peers ORDER BY id ASC"
        )
        rows = cur.fetchall()
    return [_row_to_peer(r) for r in rows]


def get_peer(db: Database, peer_id: int) -> Optional[Peer]:
    with db.connect() as conn:
        cur = conn.execute(
            "SELECT id, name, public_key, private_key_encrypted, pre_shared_key, "
            "allocated_ip, is_active, created_at "
            "FROM peers WHERE id = ?",
            (peer_id,),
        )
        row = cur.fetchone()
    return _row_to_peer(row) if row else None


def list_allocated_ips(db: Database) -> Iterable[str]:
    with db.connect() as conn:
        cur = conn.execute(
            "SELECT allocated_ip FROM peers WHERE is_active = 1"
        )
        return [r["allocated_ip"] for r in cur.fetchall()]


def create_peer(
    db: Database,
    *,
    name: str,
    public_key: str,
    private_key_encrypted: str,
    pre_shared_key: Optional[str],
    allocated_ip: str,
    is_active: bool = True,
) -> Peer:
    created_at = dt.datetime.utcnow().isoformat(timespec="seconds") + "Z"

    with db.connect() as conn:
        cur = conn.execute(
            """
            INSERT INTO peers (name, public_key, private_key_encrypted, pre_shared_key,
                               allocated_ip, is_active, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                name,
                public_key,
                private_key_encrypted,
                pre_shared_key,
                allocated_ip,
                1 if is_active else 0,
                created_at,
            ),
        )
        peer_id = cur.lastrowid
        conn.commit()

    peer = get_peer(db, peer_id)
    assert peer is not None
    return peer


def update_peer(
    db: Database,
    peer_id: int,
    *,
    name: Optional[str] = None,
    is_active: Optional[bool] = None,
) -> Optional[Peer]:
    with db.connect() as conn:
        cur = conn.execute("SELECT id, name, is_active FROM peers WHERE id = ?", (peer_id,))
        row = cur.fetchone()
        if not row:
            return None

        new_name = name if name is not None else row["name"]
        new_is_active = int(is_active) if is_active is not None else row["is_active"]

        conn.execute(
            "UPDATE peers SET name = ?, is_active = ? WHERE id = ?",
            (new_name, new_is_active, peer_id),
        )
        conn.commit()

    return get_peer(db, peer_id)


def delete_peer(db: Database, peer_id: int) -> bool:
    with db.connect() as conn:
        cur = conn.execute("DELETE FROM peers WHERE id = ?", (peer_id,))
        conn.commit()
        return cur.rowcount > 0

