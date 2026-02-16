"""ログカード — ウィジェット生成・ハンドラ・レイアウト。"""

import flet as ft

from .theme import _section_header, _styled_card
from .state import AppState


def build_log_widgets(state: AppState) -> dict:
    """ログセクションのウィジェットを生成して辞書で返す。"""
    log_field = ft.TextField(
        read_only=True,
        multiline=True,
        min_lines=8,
        max_lines=16,
        expand=True,
        border_color="#424242",
        text_style=ft.TextStyle(
            font_family="Courier New",
            size=12,
            color="#B0BEC5",
        ),
    )
    clear_log_button = ft.TextButton(
        "クリア",
        icon="delete_outline",
        style=ft.ButtonStyle(color="#EF5350"),
    )
    return {"log_field": log_field, "clear_log_button": clear_log_button}


def setup_log_handlers(state: AppState, widgets: dict):
    """ログクリアのハンドラを設定する。"""
    log_field = widgets["log_field"]
    clear_log_button = widgets["clear_log_button"]

    def on_clear_log(e):
        with state.log_lock:
            log_field.value = ""
        state.page.update()

    clear_log_button.on_click = on_clear_log


def build_log_card(widgets: dict) -> ft.Card:
    """ログカードのレイアウトを返す。"""
    return _styled_card(
        ft.Column([
            ft.Row(
                controls=[
                    _section_header("terminal", "ログ"),
                    widgets["clear_log_button"],
                ],
                alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
            ),
            ft.Divider(height=1, color="#333344"),
            widgets["log_field"],
        ]),
    )
