from __future__ import annotations

import sys
from pathlib import Path


def get_base_path() -> Path:
    """
    実行環境（開発時 or PyInstaller 単一バイナリ）に依存せず、
    静的ファイルやテンプレートの基準ディレクトリを返す。
    """
    if hasattr(sys, "_MEIPASS"):
        # PyInstaller でビルドされた場合の一時展開ディレクトリ
        return Path(getattr(sys, "_MEIPASS"))

    # 通常のスクリプト実行時: プロジェクトルート（このファイルの2階層上）を基準にする
    return Path(__file__).resolve().parent.parent.parent


def get_resource_path(*parts: str) -> Path:
    """
    プロジェクト内のリソース（templates, static, config など）への
    相対パスを組み立てるためのヘルパー。
    """
    return get_base_path().joinpath(*parts)
