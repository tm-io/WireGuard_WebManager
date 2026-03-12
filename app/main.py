from __future__ import annotations

from fastapi import FastAPI, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.templating import Jinja2Templates

from .core.config import Settings, load_settings
from .core.path_utils import get_resource_path
from .core.db import Database, init_db
from .core.logging_config import setup_logging
from .routers import auth as auth_router
from .routers import peers as peers_router
from .routers import server as server_router
from .routers import settings_api as settings_router
from .routers import docs_router


def create_app() -> FastAPI:
    setup_logging()
    settings: Settings = load_settings()

    app = FastAPI(title="WireGuard Web Manager")
    app.state.settings = settings

    # DB 初期化
    db = Database(path=get_resource_path(settings.paths.db_path))
    init_db(db)
    app.state.db = db

    templates_dir = get_resource_path("templates")
    templates = Jinja2Templates(directory=str(templates_dir))
    app.state.templates = templates

    # 画面ルートは先に登録（API より優先してマッチさせる）
    @app.get("/", include_in_schema=False)
    async def root(request: Request):
        if request.cookies.get(auth_router.SESSION_COOKIE_NAME) == "authenticated":
            return RedirectResponse(url="/dashboard")
        return RedirectResponse(url="/login")

    @app.get("/dashboard", response_class=HTMLResponse, include_in_schema=False)
    async def dashboard(request: Request):
        if request.cookies.get(auth_router.SESSION_COOKIE_NAME) != "authenticated":
            return RedirectResponse(url="/login")
        return app.state.templates.TemplateResponse(
            "dashboard.html",
            {
                "request": request,
                "username": app.state.settings.app.auth_username,
                "active_page": "dashboard",
            },
        )

    @app.get("/peers", response_class=HTMLResponse, include_in_schema=False)
    async def page_peers(request: Request):
        if request.cookies.get(auth_router.SESSION_COOKIE_NAME) != "authenticated":
            return RedirectResponse(url="/login")
        return app.state.templates.TemplateResponse(
            "peers.html",
            {
                "request": request,
                "username": app.state.settings.app.auth_username,
                "active_page": "peers",
            },
        )

    @app.get("/settings", response_class=HTMLResponse, include_in_schema=False)
    async def page_settings(request: Request):
        if request.cookies.get(auth_router.SESSION_COOKIE_NAME) != "authenticated":
            return RedirectResponse(url="/login")
        return app.state.templates.TemplateResponse(
            "settings.html",
            {
                "request": request,
                "username": app.state.settings.app.auth_username,
                "active_page": "settings",
            },
        )

    # 認証・API・ドキュメント ルーター
    app.include_router(auth_router.router)
    app.include_router(peers_router.router)
    app.include_router(server_router.router)
    app.include_router(settings_router.router)
    app.include_router(docs_router.router)

    return app


app = create_app()


if __name__ == "__main__":
    import uvicorn

    settings = load_settings()
    # systemd 等で動かすときは reload=False。開発時は uvicorn --reload を直接使う。
    uvicorn.run(
        "app.main:app",
        host=settings.app.host,
        port=settings.app.port,
        reload=False,
    )
