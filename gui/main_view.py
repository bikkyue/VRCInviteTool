"""メイン画面 — レイアウト組み立て・データ取得。"""

import threading

import flet as ft
from vrchatapi.exceptions import UnauthorizedException

from auth import logout
from .theme import COLOR_PAGE_BG, _title_banner
from .state import AppState
from .sections.instance_section import fetch_favorite_worlds, build_instance_card
from .sections.invite_section import fetch_all_friends, build_invite_card
from .log_section import build_log_card


def _show_loading_dialog(state: AppState, message: str) -> ft.AlertDialog:
    dlg = ft.AlertDialog(
        modal=True,
        content=ft.Column(
            controls=[
                ft.ProgressRing(width=40, height=40, stroke_width=3),
                ft.Text(message, size=14),
            ],
            horizontal_alignment=ft.CrossAxisAlignment.CENTER,
            spacing=16,
            tight=True,
        ),
    )
    state.page.open(dlg)
    state.page.update()
    return dlg


def fetch_data_with_loading(state: AppState):
    """フレンド一覧とお気に入りワールドをローディングダイアログ付きで取得する。"""
    def run():
        dlg = _show_loading_dialog(state, "フレンド一覧・ワールドを取得中...")
        session_expired = False
        try:
            state.friends = fetch_all_friends(state.api_client)
            state.favorite_worlds = fetch_favorite_worlds(state.api_client)
            state.log_queue.put(
                f"フレンド {len(state.friends)} 人、"
                f"お気に入りワールド {len(state.favorite_worlds)} 件を取得しました。\n"
            )
        except UnauthorizedException:
            session_expired = True
        except Exception as ex:
            state.log_queue.put(f"データ取得エラー: {ex}\n")
        finally:
            state.page.close(dlg)
            state.page.update()
        if session_expired:
            state.handle_session_expiry()

    threading.Thread(target=run, daemon=True).start()


def show_main_view(state: AppState, instance_w: dict, invite_w: dict, log_w: dict):
    """メイン画面全体を組み立てて表示する。"""
    logged_in_name_text = ft.Text(state.display_name, size=13, color="white")

    logout_button = ft.TextButton(
        "ログアウト",
        icon="logout",
        style=ft.ButtonStyle(color="white"),
    )
    refresh_button = ft.IconButton(
        icon="refresh",
        icon_color="white",
        tooltip="フレンド・ワールド情報を更新",
        on_click=lambda e: fetch_data_with_loading(state),
    )

    def on_logout_click(e):
        logout(clear_session=state.clear_session)
        state.api_client = None
        state.display_name = ""
        if state._show_login_fn:
            state._show_login_fn()

    logout_button.on_click = on_logout_click

    # ワールド情報の初期取得（world_id_field に値がある場合）
    if instance_w["world_id_field"].value and "fetch_world_info" in instance_w:
        instance_w["fetch_world_info"](instance_w["world_id_field"].value)

    state.page.scroll = ft.ScrollMode.AUTO
    state.page.views.clear()
    state.page.views.append(
        ft.View(
            route="/main",
            bgcolor=COLOR_PAGE_BG,
            padding=ft.padding.symmetric(horizontal=20, vertical=16),
            scroll=ft.ScrollMode.AUTO,
            controls=[
                _title_banner(
                    right_content=ft.Row(
                        controls=[logged_in_name_text, refresh_button, logout_button],
                        spacing=4,
                        vertical_alignment=ft.CrossAxisAlignment.CENTER,
                    )
                ),
                build_instance_card(instance_w),
                build_invite_card(invite_w),
                build_log_card(log_w),
            ],
        )
    )
    state.page.update()

    # フレンド・ワールドデータをバックグラウンドで自動取得
    fetch_data_with_loading(state)
