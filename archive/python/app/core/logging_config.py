"""
アプリ用ログ設定。syslog にエラーを送り、journalctl でも確認できるようにする。
"""
from __future__ import annotations

import logging
import logging.handlers


def setup_logging() -> None:
    """ルートロガーに syslog ハンドラを追加する。"""
    root = logging.getLogger()
    if any(h for h in root.handlers if isinstance(h, logging.handlers.SysLogHandler)):
        return
    try:
        handler = logging.handlers.SysLogHandler(address="/dev/log", facility=logging.handlers.SysLogHandler.LOG_DAEMON)
        handler.setFormatter(logging.Formatter("wg-manager[%(process)d]: %(name)s %(levelname)s %(message)s"))
        root.addHandler(handler)
        root.setLevel(logging.INFO)
    except OSError:
        # /dev/log が無い環境（例: コンテナ）では無視
        pass
