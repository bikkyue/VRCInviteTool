"""ログイン画面 — ウィジェット生成・ハンドラ・表示・自動ログイン。"""

import threading

import flet as ft
from vrchatapi.exceptions import UnauthorizedException

from auth import create_api_client, login, try_session_login
from .theme import COLOR_PRIMARY, COLOR_ACCENT, COLOR_PAGE_BG, _section_header, _styled_card, _title_banner
from .state import AppState


def build_login_widgets(state: AppState) -> dict:
    """ログイン画面のウィジェットを生成して辞書で返す。"""
    username_field = ft.TextField(
        label="ユーザー名",
        value=state.load_username(),
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
        autofocus=True,
    )
    password_field = ft.TextField(
        label="パスワード",
        value="",
        password=True,
        can_reveal_password=True,
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
    )
    login_error_text = ft.Text("", color="#EF5350", size=13, visible=False)
    login_loading = ft.ProgressRing(width=20, height=20, stroke_width=2, visible=False)
    login_button = ft.ElevatedButton(
        "ログイン",
        icon="login",
        color="white",
        bgcolor=COLOR_PRIMARY,
        style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8)),
    )
    return {
        "username_field": username_field,
        "password_field": password_field,
        "login_error_text": login_error_text,
        "login_loading": login_loading,
        "login_button": login_button,
    }


def show_login_view(state: AppState, widgets: dict):
    """ログイン画面を表示する。"""
    w = widgets
    w["login_error_text"].value = ""
    w["login_error_text"].visible = False
    w["login_loading"].visible = False
    w["login_button"].disabled = False

    state.page.scroll = ft.ScrollMode.AUTO
    state.page.views.clear()
    state.page.views.append(
        ft.View(
            route="/login",
            bgcolor=COLOR_PAGE_BG,
            padding=ft.padding.symmetric(horizontal=20, vertical=16),
            scroll=ft.ScrollMode.AUTO,
            controls=[
                _title_banner(),
                _styled_card(
                    ft.Column([
                        _section_header("lock", "ログイン"),
                        ft.Divider(height=1, color="#333344"),
                        w["username_field"],
                        w["password_field"],
                        ft.Row(
                            controls=[w["login_button"], w["login_loading"]],
                            spacing=12,
                            vertical_alignment=ft.CrossAxisAlignment.CENTER,
                        ),
                        w["login_error_text"],
                    ], spacing=14),
                ),
            ],
        )
    )
    state.page.update()


def setup_login_handlers(state: AppState, widgets: dict, on_login_success):
    """ログインボタン・Enter キーのイベントハンドラを設定する。
    on_login_success: ログイン成功時に呼ぶコールバック（メイン画面遷移）。
    """
    w = widgets

    def on_login_click(e):
        def run():
            uname = w["username_field"].value.strip()
            pwd = w["password_field"].value

            if not uname or not pwd:
                w["login_error_text"].value = "ユーザー名とパスワードを入力してください。"
                w["login_error_text"].visible = True
                state.page.update()
                return

            w["login_button"].disabled = True
            w["login_loading"].visible = True
            w["login_error_text"].visible = False
            state.page.update()

            try:
                api_client = create_api_client(username=uname, password=pwd)
                ok = login(
                    api_client,
                    input_fn=state.two_factor_input_fn,
                    save_session=state.save_session,
                    load_session=state.load_session,
                    clear_session=state.clear_session,
                )
                if ok:
                    state.save_username(uname)

                    from vrchatapi.api import authentication_api as _auth_api_mod
                    auth_api = _auth_api_mod.AuthenticationApi(api_client)
                    current_user = auth_api.get_current_user()
                    state.api_client = api_client
                    state.display_name = current_user.display_name
                    state.user_id = current_user.id

                    on_login_success()
                else:
                    w["login_error_text"].value = "ログインに失敗しました。"
                    w["login_error_text"].visible = True
                    w["login_button"].disabled = False
                    w["login_loading"].visible = False
                    state.page.update()
            except Exception as ex:
                w["login_error_text"].value = f"エラー: {ex}"
                w["login_error_text"].visible = True
                w["login_button"].disabled = False
                w["login_loading"].visible = False
                state.page.update()

        threading.Thread(target=run, daemon=True).start()

    w["login_button"].on_click = on_login_click
    w["password_field"].on_submit = on_login_click


def startup_auto_login(state: AppState, widgets: dict, on_login_success):
    """起動時セッション自動ログインを試みる。"""
    w = widgets

    def run():
        w["login_button"].disabled = True
        w["login_loading"].visible = True
        w["login_error_text"].value = "セッション確認中..."
        w["login_error_text"].color = COLOR_ACCENT
        w["login_error_text"].visible = True
        state.page.update()

        try:
            uname = state.load_username()
            if uname:
                api_client = create_api_client(username=uname)
                display_name = try_session_login(
                    api_client,
                    load_session=state.load_session,
                    clear_session=state.clear_session,
                )
                if display_name:
                    from vrchatapi.api import authentication_api as _auth_api_mod
                    auth_api = _auth_api_mod.AuthenticationApi(api_client)
                    current_user = auth_api.get_current_user()
                    state.api_client = api_client
                    state.display_name = display_name
                    state.user_id = current_user.id
                    on_login_success()
                    return
        except Exception:
            pass

        # セッションが無効 → ログイン画面をリセット
        w["login_button"].disabled = False
        w["login_loading"].visible = False
        w["login_error_text"].value = ""
        w["login_error_text"].color = "#EF5350"
        w["login_error_text"].visible = False
        state.page.update()

    threading.Thread(target=run, daemon=True).start()
