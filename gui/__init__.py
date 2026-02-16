"""VRCInviteTool - Flet GUI エントリポイント。"""

import flet as ft

from .state import AppState
from .login_view import build_login_widgets, show_login_view, setup_login_handlers, startup_auto_login
from .main_view import show_main_view
from .sections.instance_section import build_instance_widgets, setup_instance_handlers
from .sections.invite_section import build_invite_widgets, setup_invite_handlers
from .log_section import build_log_widgets, setup_log_handlers


def main(page: ft.Page):
    page.title = "VRCInviteTool"
    page.window.width = 620
    page.window.resizable = False
    page.bgcolor = "#121218"
    page.theme_mode = ft.ThemeMode.DARK
    page.padding = ft.padding.symmetric(horizontal=20, vertical=16)

    state = AppState(page)

    # ウィジェット生成
    login_w = build_login_widgets(state)
    instance_w = build_instance_widgets(state)
    invite_w = build_invite_widgets(state)
    log_w = build_log_widgets(state)

    # ログフィールドを AppState に登録（append_log が使えるようになる）
    state.set_log_field(log_w["log_field"])

    # ログフラッシュスレッド起動
    state.start_log_flush()

    # アクションボタンを AppState に登録（set_buttons_disabled で一括制御）
    state.register_action_button(instance_w["create_button"])
    state.register_action_button(invite_w["invite_button"])
    state.register_action_button(invite_w["self_invite_button"])

    # インスタンス作成結果を招待セクションに渡すコールバック
    def on_instance_created(location: str):
        invite_w["invite_instance_id_field"].value = location
        page.update()

    # ハンドラ設定
    setup_instance_handlers(state, instance_w, on_instance_created=on_instance_created)
    setup_invite_handlers(state, invite_w)
    setup_log_handlers(state, log_w)

    # メイン画面遷移コールバック
    def go_main():
        show_main_view(state, instance_w, invite_w, log_w)

    setup_login_handlers(state, login_w, on_login_success=go_main)

    # ログイン画面表示コールバックを AppState に登録
    def _show_login():
        show_login_view(state, login_w)

    def _set_session_expired_message():
        login_w["login_error_text"].value = "セッションが切れました。再度ログインしてください。"
        login_w["login_error_text"].color = ft.colors.AMBER
        login_w["login_error_text"].visible = True
        page.update()

    state._show_login_fn = _show_login
    state._session_expired_message_fn = _set_session_expired_message

    # 初期画面表示 + セッション自動ログイン
    show_login_view(state, login_w)
    startup_auto_login(state, login_w, on_login_success=go_main)
