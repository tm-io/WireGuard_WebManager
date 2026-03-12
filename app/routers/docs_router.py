"""
docs フォルダ内の Markdown を一覧・表示する。認証済みユーザーのみ。
"""
from __future__ import annotations

from pathlib import Path

import markdown
from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.core.path_utils import get_resource_path
from app.routers import auth as auth_router

router = APIRouter(tags=["docs"])

DOCS_DIR = get_resource_path("docs")

# 一覧に出すファイルと表示名（README は目次として別扱い）
DOC_ENTRIES = [
    ("README.md", "マニュアル目次"),
    ("01-setup.md", "01 - 初回セットアップ"),
    ("02-config.md", "02 - config.yaml カスタマイズ"),
    ("03-operation.md", "03 - 運用イメージ"),
    ("WORKER.md", "Worker による権限分離"),
]


def _require_auth(request: Request) -> None:
    if request.cookies.get(auth_router.SESSION_COOKIE_NAME) != "authenticated":
        raise HTTPException(status_code=401, detail="ログインしてください。")


def _safe_doc_path(name: str) -> Path | None:
    """名前から安全な docs 直下の .md パスを返す。不正なら None。"""
    name = name.strip().replace("\\", "/")
    if not name or ".." in name or "/" in name:
        return None
    if not name.endswith(".md"):
        name = name + ".md"
    path = (DOCS_DIR / name).resolve()
    docs_resolved = DOCS_DIR.resolve()
    try:
        if not path.is_file() or path.parent != docs_resolved:
            return None
    except Exception:
        return None
    return path


@router.get("/manual", response_class=HTMLResponse, include_in_schema=False)
async def page_docs_list(request: Request):
    _require_auth(request)
    templates: Jinja2Templates = request.app.state.templates
    entries = []
    for filename, title in DOC_ENTRIES:
        path = DOCS_DIR / filename
        if path.is_file():
            entries.append({"filename": filename, "name": filename.replace(".md", ""), "title": title})
    return templates.TemplateResponse(
        "docs_list.html",
        {
            "request": request,
            "username": request.app.state.settings.app.auth_username,
            "active_page": "docs",
            "entries": entries,
        },
    )


@router.get("/manual/view/{name}", response_class=HTMLResponse, include_in_schema=False)
async def page_docs_view(request: Request, name: str):
    _require_auth(request)
    path = _safe_doc_path(name)
    if not path:
        raise HTTPException(status_code=404, detail="ドキュメントが見つかりません。")
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError:
        raise HTTPException(status_code=404, detail="ドキュメントを読み込めませんでした。")
    html_body = markdown.markdown(raw, extensions=["extra"])
    templates: Jinja2Templates = request.app.state.templates
    return templates.TemplateResponse(
        "docs_view.html",
        {
            "request": request,
            "username": request.app.state.settings.app.auth_username,
            "active_page": "docs",
            "title": path.stem,
            "html_body": html_body,
        },
    )
