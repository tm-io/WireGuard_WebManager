from __future__ import annotations

import sqlite3
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass
class Database:
    path: Path

    def connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(self.path)
        conn.row_factory = sqlite3.Row
        return conn


def init_db(db: Database) -> None:
    """
    peers テーブルを作成する。
    """
    db.path.parent.mkdir(parents=True, exist_ok=True)

    with db.connect() as conn:
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS peers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                public_key TEXT NOT NULL UNIQUE,
                private_key_encrypted TEXT NOT NULL,
                pre_shared_key TEXT,
                allocated_ip TEXT NOT NULL UNIQUE,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            )
            """
        )
        conn.commit()

