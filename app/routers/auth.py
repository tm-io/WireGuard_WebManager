from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.templating import Jinja2Templates
from starlette import status

from app.core.config import Settings


router = APIRouter(tags=["auth"])


def get_templates() -> Jinja2Templates:
    # main.py 側で Jinja2Templates を共有しても良いが、
    # Phase 1 ではシンプルにここで生成する。
    from app.core.path_utils import get_resource_path
    from fastapi.templating import Jinja2Templates as _Jinja2Templates

    templates_dir = get_resource_path("templates")
    return _Jinja2Templates(directory=str(templates_dir))


def get_settings(request: Request) -> Settings:
    # main.py で app.state.settings に格納しておき、それを参照する想定
    return request.app.state.settings


SESSION_COOKIE_NAME = "wgwm_session"


def _is_authenticated(request: Request) -> bool:
    return request.cookies.get(SESSION_COOKIE_NAME) == "authenticated"


def require_login(request: Request) -> None:
    if not _is_authenticated(request):
        # 未ログインならログイン画面へリダイレクト
        raise RedirectResponse(url="/login", status_code=status.HTTP_303_SEE_OTHER)


@router.get("/login", response_class=HTMLResponse)
async def login_form(request: Request):
    templates = get_templates()
    return templates.TemplateResponse(
        "login.html",
        {
            "request": request,
            "error": None,
        },
    )


@router.post("/login")
async def login(
    request: Request,
    username: Annotated[str, Form(...)]
    ,
    password: Annotated[str, Form(...)],
):
    templates = get_templates()
    settings = get_settings(request)

    if (
        username == settings.app.auth_username
        and password == settings.app.auth_password
    ):
        response = RedirectResponse(
            url="/dashboard", status_code=status.HTTP_303_SEE_OTHER
        )
        # 非常にシンプルなセッション実装（Phase 1用）
        response.set_cookie(
            key=SESSION_COOKIE_NAME,
            value="authenticated",
            httponly=True,
            secure=False,  # 本番では True + HTTPS を推奨
            samesite="lax",
        )
        return response

    # 認証失敗
    return templates.TemplateResponse(
        "login.html",
        {
            "request": request,
            "error": "ユーザー名またはパスワードが正しくありません。",
        },
        status_code=status.HTTP_401_UNAUTHORIZED,
    )


@router.get("/logout")
async def logout():
    response = RedirectResponse(url="/login", status_code=status.HTTP_303_SEE_OTHER)
    response.delete_cookie(SESSION_COOKIE_NAME)
    return response
