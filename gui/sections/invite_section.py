"""招待セクション — ウィジェット・ハンドラ・レイアウト。"""

import sys
import time
import base64
import threading

import flet as ft
import requests
from vrchatapi.api import users_api
from vrchatapi.exceptions import ApiException, UnauthorizedException

from invite_user import invite_user, invite_myself
from gui.theme import (
    COLOR_PRIMARY, COLOR_ACCENT, COLOR_HEADER_TEXT, COLOR_INVITE,
    _DISABLED_BGCOLOR, _DISABLED_COLOR,
    _section_header, _styled_card,
)
from gui.state import AppState


def fetch_all_friends(api_client) -> list:
    """全フレンドを取得する (オンライン + オフライン)。"""
    from vrchatapi.api import friends_api
    api = friends_api.FriendsApi(api_client)
    all_friends = []
    for offline in [False, True]:
        offset = 0
        while True:
            batch = api.get_friends(offset=offset, n=50, offline=offline)
            all_friends.extend(batch)
            if len(batch) < 50:
                break
            offset += 50
    return all_friends


def build_invite_widgets(state: AppState) -> dict:
    """招待セクションのウィジェットを生成して辞書で返す。"""
    user_icon = ft.Image(
        src="",
        width=80,
        height=80,
        fit=ft.ImageFit.COVER,
        border_radius=40,
        visible=False,
    )
    user_display_name = ft.Text(
        "",
        size=14,
        weight=ft.FontWeight.BOLD,
        color=COLOR_HEADER_TEXT,
        visible=False,
    )
    user_info_loading = ft.ProgressRing(width=20, height=20, stroke_width=2, visible=False)
    user_info_error = ft.Text("", size=12, color="#EF5350", visible=False)

    invite_user_id_field = ft.TextField(
        label="ユーザーID (usr_...)",
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
        visible=False,
    )
    friend_search_field = ft.TextField(
        label="フレンド検索 (名前 or ID)",
        prefix_icon="search",
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
        expand=True,
    )
    friend_dropdown = ft.ListView(height=200, spacing=1, visible=False)
    selected_chips_row = ft.Row(controls=[], wrap=True, spacing=4, run_spacing=4)
    selection_counter = ft.Text("選択中: 0 / 20", size=12, color=COLOR_ACCENT, visible=False)
    invite_instance_id_field = ft.TextField(
        label="インスタンスID",
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
    )

    invite_button = ft.ElevatedButton(
        "招待する",
        icon="person_add",
        style=ft.ButtonStyle(
            shape=ft.RoundedRectangleBorder(radius=8),
            bgcolor={
                ft.ControlState.DEFAULT: COLOR_INVITE,
                ft.ControlState.DISABLED: _DISABLED_BGCOLOR,
            },
            color={
                ft.ControlState.DEFAULT: "white",
                ft.ControlState.DISABLED: _DISABLED_COLOR,
            },
            icon_color={
                ft.ControlState.DEFAULT: "white",
                ft.ControlState.DISABLED: _DISABLED_COLOR,
            },
        ),
    )
    self_invite_button = ft.ElevatedButton(
        "自分に招待を送る",
        icon="person",
        style=ft.ButtonStyle(
            shape=ft.RoundedRectangleBorder(radius=8),
            bgcolor={
                ft.ControlState.DEFAULT: "#0097A7",
                ft.ControlState.DISABLED: _DISABLED_BGCOLOR,
            },
            color={
                ft.ControlState.DEFAULT: "white",
                ft.ControlState.DISABLED: _DISABLED_COLOR,
            },
            icon_color={
                ft.ControlState.DEFAULT: "white",
                ft.ControlState.DISABLED: _DISABLED_COLOR,
            },
        ),
    )

    return {
        "user_icon": user_icon,
        "user_display_name": user_display_name,
        "user_info_loading": user_info_loading,
        "user_info_error": user_info_error,
        "invite_user_id_field": invite_user_id_field,
        "friend_search_field": friend_search_field,
        "friend_dropdown": friend_dropdown,
        "selected_chips_row": selected_chips_row,
        "selection_counter": selection_counter,
        "invite_instance_id_field": invite_instance_id_field,
        "invite_button": invite_button,
        "self_invite_button": self_invite_button,
        # ミュータブルな選択状態
        "selected_friend_ids": set(),
        "selected_friend_names": {},
    }


def setup_invite_handlers(state: AppState, widgets: dict):
    """フレンド検索・ユーザー情報取得・招待送信のハンドラを設定する。"""
    user_icon = widgets["user_icon"]
    user_display_name = widgets["user_display_name"]
    user_info_loading = widgets["user_info_loading"]
    user_info_error = widgets["user_info_error"]
    friend_search_field = widgets["friend_search_field"]
    friend_dropdown = widgets["friend_dropdown"]
    selected_chips_row = widgets["selected_chips_row"]
    selection_counter = widgets["selection_counter"]
    invite_instance_id_field = widgets["invite_instance_id_field"]
    invite_button = widgets["invite_button"]
    self_invite_button = widgets["self_invite_button"]
    selected_friend_ids: set = widgets["selected_friend_ids"]
    selected_friend_names: dict = widgets["selected_friend_names"]

    def fetch_user_info(user_id: str):
        """ユーザー情報を取得して GUI に表示する。"""
        if not user_id or not user_id.startswith("usr_"):
            user_icon.visible = False
            user_display_name.visible = False
            user_info_error.visible = False
            state.page.update()
            return

        def run():
            try:
                user_info_loading.visible = True
                user_icon.visible = False
                user_display_name.visible = False
                user_info_error.visible = False
                state.page.update()

                if state.api_client is None:
                    user_info_loading.visible = False
                    user_info_error.value = "ログインしていません"
                    user_info_error.visible = True
                    state.page.update()
                    return

                api = users_api.UsersApi(state.api_client)
                user = api.get_user(user_id)
                # state.log_queue.put(f"ユーザー名: {user.display_name}\n")

                icon_url = (
                    user.profile_pic_override
                    or user.profile_pic_override_thumbnail
                    or user.current_avatar_image_url
                    or user.current_avatar_thumbnail_image_url
                )
                if icon_url:
                    try:
                        cookies = {}
                        for cookie in state.api_client.rest_client.cookie_jar:
                            cookies[cookie.name] = cookie.value
                        headers = {"User-Agent": state.api_client.user_agent}
                        response = requests.get(icon_url, cookies=cookies, headers=headers, timeout=10)
                        response.raise_for_status()
                        base64_image = base64.b64encode(response.content).decode()
                        user_icon.src_base64 = base64_image
                        user_icon.visible = True
                    except Exception:
                        user_icon.visible = False
                else:
                    user_icon.visible = False

                user_display_name.value = user.display_name
                user_display_name.visible = True
                user_info_loading.visible = False
                user_info_error.visible = False
                state.page.update()

            except UnauthorizedException:
                user_info_loading.visible = False
                state.page.update()
                state.handle_session_expiry()
            except ApiException as e:
                user_info_loading.visible = False
                user_icon.visible = False
                user_display_name.visible = False
                user_info_error.value = "ユーザーが見つかりません" if e.status == 404 else f"エラー: {e.status}"
                user_info_error.visible = True
                state.page.update()
            except Exception as e:
                user_info_loading.visible = False
                user_icon.visible = False
                user_display_name.visible = False
                user_info_error.value = f"エラー: {str(e)}"
                user_info_error.visible = True
                state.page.update()

        threading.Thread(target=run, daemon=True).start()

    def rebuild_chips():
        selected_chips_row.controls.clear()
        for fid, fname in selected_friend_names.items():
            selected_chips_row.controls.append(
                ft.Chip(
                    label=ft.Text(fname, size=12),
                    on_delete=lambda e, fid=fid: on_chip_delete(fid),
                    bgcolor="#333355",
                )
            )
        selection_counter.visible = len(selected_friend_ids) > 0

    def on_chip_delete(friend_id):
        was_first = bool(selected_friend_names) and next(iter(selected_friend_names)) == friend_id
        selected_friend_ids.discard(friend_id)
        selected_friend_names.pop(friend_id, None)
        rebuild_chips()
        selection_counter.value = f"選択中: {len(selected_friend_ids)} / 20"
        if not selected_friend_ids:
            user_icon.visible = False
            user_display_name.visible = False
            user_info_error.visible = False
            user_info_loading.visible = False
        elif was_first:
            fetch_user_info(next(iter(selected_friend_names)))
        state.page.update()

    def on_friend_select(friend):
        if friend.id in selected_friend_ids:
            friend_dropdown.visible = False
            friend_search_field.value = ""
            state.page.update()
            return
        if len(selected_friend_ids) >= 20:
            state.log_queue.put("選択上限(20人)に達しています。\n")
            return
        is_first = len(selected_friend_ids) == 0
        selected_friend_ids.add(friend.id)
        selected_friend_names[friend.id] = friend.display_name
        rebuild_chips()
        friend_search_field.value = ""
        friend_dropdown.visible = False
        selection_counter.value = f"選択中: {len(selected_friend_ids)} / 20"
        if is_first:
            fetch_user_info(friend.id)
        state.page.update()

    def build_friend_dropdown_item(friend) -> ft.Container:
        return ft.Container(
            content=ft.Column([
                ft.Text(friend.display_name, size=14, weight=ft.FontWeight.BOLD, color="white"),
                ft.Text(friend.id, size=11, color="#888888"),
            ], spacing=2),
            padding=ft.padding.symmetric(vertical=6, horizontal=10),
            border_radius=4,
            bgcolor="#252535",
            on_click=lambda e, f=friend: on_friend_select(f),
            ink=True,
        )

    def on_friend_search_change(e):
        query = e.control.value.strip().lower()
        if not query:
            friend_dropdown.visible = False
            state.page.update()
            return
        filtered = [
            f for f in state.friends
            if query in f.display_name.lower() or query in f.id.lower()
        ]
        friend_dropdown.controls.clear()
        for f in filtered[:20]:
            friend_dropdown.controls.append(build_friend_dropdown_item(f))
        friend_dropdown.visible = len(filtered) > 0
        state.page.update()

    def on_friend_search_focus(e):
        if state.friends and not friend_search_field.value:
            friend_dropdown.controls.clear()
            for f in state.friends[:20]:
                friend_dropdown.controls.append(build_friend_dropdown_item(f))
            friend_dropdown.visible = True
            state.page.update()

    def on_friend_search_blur(e):
        def _hide():
            time.sleep(0.2)
            friend_dropdown.visible = False
            state.page.update()
        threading.Thread(target=_hide, daemon=True).start()

    def on_invite_click(e):
        def run():
            sys.stdout = state.gui_output
            try:
                state.set_buttons_disabled(True)
                iid = invite_instance_id_field.value.strip()
                if not iid:
                    state.log_queue.put("エラー: インスタンスIDを入力してください。\n")
                    return
                if state.api_client is None:
                    state.log_queue.put("エラー: ログインしていません。\n")
                    return

                targets = list(selected_friend_ids)
                if not targets:
                    uid = friend_search_field.value.strip()
                    if not uid:
                        state.log_queue.put("エラー: ユーザーを選択または入力してください。\n")
                        return
                    targets = [uid]

                total = len(targets)
                state.log_queue.put(f"{total}人に招待を送信します...\n")
                for i, uid in enumerate(targets):
                    fname = selected_friend_names.get(uid, uid)
                    state.log_queue.put(f"[{i+1}/{total}] {fname} に招待中...\n")
                    invite_user(state.api_client, uid, iid)
                    if i < total - 1:
                        time.sleep(1)
                state.log_queue.put("全ての招待を送信しました。\n")

                # 選択をリセット
                selected_friend_ids.clear()
                selected_friend_names.clear()
                rebuild_chips()
                selection_counter.value = "選択中: 0 / 20"
                user_icon.visible = False
                user_display_name.visible = False
                user_info_error.visible = False
                user_info_loading.visible = False
                friend_search_field.value = ""
                state.page.update()
            except UnauthorizedException:
                state.handle_session_expiry()
            except Exception as ex:
                state.log_queue.put(f"エラー: {ex}\n")
            finally:
                sys.stdout = state.original_stdout
                state.set_buttons_disabled(False)

        threading.Thread(target=run, daemon=True).start()

    def on_self_invite_click(e):
        def run():
            sys.stdout = state.gui_output
            try:
                state.set_buttons_disabled(True)
                iid = invite_instance_id_field.value.strip()
                if not iid:
                    state.log_queue.put("エラー: インスタンスIDを入力してください。\n")
                    return
                if state.api_client is None:
                    state.log_queue.put("エラー: ログインしていません。\n")
                    return
                invite_myself(state.api_client, iid)
            except UnauthorizedException:
                state.handle_session_expiry()
            except Exception as ex:
                state.log_queue.put(f"エラー: {ex}\n")
            finally:
                sys.stdout = state.original_stdout
                state.set_buttons_disabled(False)

        threading.Thread(target=run, daemon=True).start()

    friend_search_field.on_change = on_friend_search_change
    friend_search_field.on_focus = on_friend_search_focus
    friend_search_field.on_blur = on_friend_search_blur
    invite_button.on_click = on_invite_click
    self_invite_button.on_click = on_self_invite_click


def build_invite_card(widgets: dict) -> ft.Card:
    """招待セクションの Card レイアウトを組み立てて返す。"""
    return _styled_card(
        ft.Column([
            _section_header("mail", "招待"),
            ft.Divider(height=1, color="#333344"),
            ft.Row([
                ft.Column([
                    widgets["friend_search_field"],
                    widgets["friend_dropdown"],
                    widgets["selected_chips_row"],
                    widgets["selection_counter"],
                    widgets["invite_instance_id_field"],
                    ft.Row([
                        widgets["invite_button"],
                        widgets["self_invite_button"],
                    ], spacing=8),
                ], expand=True, spacing=8),
                ft.Container(
                    content=ft.Column([
                        widgets["user_info_loading"],
                        widgets["user_icon"],
                        widgets["user_display_name"],
                        widgets["user_info_error"],
                    ], horizontal_alignment=ft.CrossAxisAlignment.CENTER),
                    width=120,
                    padding=ft.padding.only(left=10),
                ),
            ]),
        ]),
    )
