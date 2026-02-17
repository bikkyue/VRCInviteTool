"""インスタンス作成セクション — ウィジェット・ハンドラ・レイアウト。"""

import sys
import time
import base64
import threading

import flet as ft
import requests
from vrchatapi.api import worlds_api
from vrchatapi.exceptions import ApiException, UnauthorizedException

from create_instance import create_instance
from gui.theme import (
    COLOR_PRIMARY, COLOR_ACCENT, COLOR_HEADER_TEXT,
    _DISABLED_BGCOLOR, _DISABLED_COLOR,
    _section_header, _styled_card,
)
from gui.state import AppState


def fetch_favorite_worlds(api_client) -> list:
    """お気に入りワールドを全件取得する (ユーザーの全ワールドグループ対象)。"""
    from vrchatapi.api import favorites_api
    fav_api = favorites_api.FavoritesApi(api_client)
    w_api = worlds_api.WorldsApi(api_client)

    groups = fav_api.get_favorite_groups(n=100)
    world_groups = [g for g in groups if str(g.type) in ("world", "vrcPlusWorld")]

    all_worlds = []
    for group in world_groups:
        offset = 0
        while True:
            batch = w_api.get_favorited_worlds(tag=group.name, offset=offset, n=50)
            all_worlds.extend(batch)
            if len(batch) < 50:
                break
            offset += 50
    return all_worlds


def build_instance_widgets(state: AppState) -> dict:
    """インスタンス作成セクションのウィジェットを生成して辞書で返す。"""
    world_thumbnail = ft.Image(
        src="",
        width=200,
        height=150,
        fit=ft.ImageFit.COVER,
        border_radius=8,
        visible=False,
    )
    world_name_text = ft.Text(
        "",
        size=14,
        weight=ft.FontWeight.BOLD,
        color=COLOR_HEADER_TEXT,
        visible=False,
    )
    world_info_loading = ft.ProgressRing(width=20, height=20, stroke_width=2, visible=False)
    world_info_error = ft.Text("", size=12, color="#EF5350", visible=False)

    world_id_field = ft.TextField(
        label="ワールドID",
        value="",
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
    )
    world_search_field = ft.TextField(
        label="ワールド検索 (名前 or ID)",
        prefix_icon="search",
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
        expand=True,
    )
    world_dropdown = ft.ListView(height=200, spacing=1, visible=False)

    instance_type_dropdown = ft.Dropdown(
        label="インスタンスタイプ",
        value="public",
        options=[
            ft.dropdown.Option("public", "Public"),
            ft.dropdown.Option("friends", "Friends"),
            ft.dropdown.Option("hidden", "Friends+"),
            ft.dropdown.Option("invite", "Invite"),
            ft.dropdown.Option("invite_plus", "Invite+"),
        ],
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
    )
    instance_region_dropdown = ft.Dropdown(
        label="リージョン",
        value="jp",
        options=[
            ft.dropdown.Option("jp"),
            ft.dropdown.Option("us"),
            ft.dropdown.Option("use"),
            ft.dropdown.Option("eu"),
        ],
        expand=True,
        border_color=COLOR_PRIMARY,
        focused_border_color=COLOR_ACCENT,
    )

    location_field = ft.TextField(
        label="インスタンス場所",
        read_only=True,
        expand=True,
        border_color=COLOR_PRIMARY,
    )
    location_row = ft.Row(
        controls=[
            location_field,
            ft.IconButton(
                icon="content_copy",
                icon_color=COLOR_ACCENT,
                tooltip="コピー",
                on_click=lambda e: state.page.set_clipboard(location_field.value),
            ),
        ],
        visible=False,
    )

    create_button = ft.ElevatedButton(
        "インスタンスを作成",
        icon="add_circle_outline",
        style=ft.ButtonStyle(
            shape=ft.RoundedRectangleBorder(radius=8),
            bgcolor={
                ft.ControlState.DEFAULT: COLOR_PRIMARY,
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
        "world_thumbnail": world_thumbnail,
        "world_name_text": world_name_text,
        "world_info_loading": world_info_loading,
        "world_info_error": world_info_error,
        "world_id_field": world_id_field,
        "world_search_field": world_search_field,
        "world_dropdown": world_dropdown,
        "instance_type_dropdown": instance_type_dropdown,
        "instance_region_dropdown": instance_region_dropdown,
        "location_field": location_field,
        "location_row": location_row,
        "create_button": create_button,
    }


def setup_instance_handlers(state: AppState, widgets: dict, on_instance_created=None):
    """ワールド検索・ワールド情報取得・インスタンス作成のハンドラを設定する。
    on_instance_created(location): インスタンス作成成功時に呼ぶコールバック。
    """
    world_thumbnail = widgets["world_thumbnail"]
    world_name_text = widgets["world_name_text"]
    world_info_loading = widgets["world_info_loading"]
    world_info_error = widgets["world_info_error"]
    world_id_field = widgets["world_id_field"]
    world_search_field = widgets["world_search_field"]
    world_dropdown = widgets["world_dropdown"]
    instance_type_dropdown = widgets["instance_type_dropdown"]
    instance_region_dropdown = widgets["instance_region_dropdown"]
    location_field = widgets["location_field"]
    location_row = widgets["location_row"]
    create_button = widgets["create_button"]

    def fetch_world_info(world_id: str):
        """ワールド情報を取得して GUI に表示する。"""
        if not world_id or not world_id.startswith("wrld_"):
            world_thumbnail.visible = False
            world_name_text.visible = False
            world_info_error.visible = False
            state.page.update()
            return

        def run():
            try:
                world_info_loading.visible = True
                world_thumbnail.visible = False
                world_name_text.visible = False
                world_info_error.visible = False
                state.page.update()

                if state.api_client is None:
                    world_info_loading.visible = False
                    world_info_error.value = "ログインしていません"
                    world_info_error.visible = True
                    state.page.update()
                    return

                api = worlds_api.WorldsApi(state.api_client)
                world = api.get_world(world_id)
                # state.log_queue.put(f"ワールド名: {world.name}\n")

                thumbnail_url = world.thumbnail_image_url or world.image_url
                if thumbnail_url:
                    try:
                        cookies = {}
                        for cookie in state.api_client.rest_client.cookie_jar:
                            cookies[cookie.name] = cookie.value
                        headers = {"User-Agent": state.api_client.user_agent}
                        response = requests.get(thumbnail_url, cookies=cookies, headers=headers, timeout=10)
                        response.raise_for_status()
                        base64_image = base64.b64encode(response.content).decode()
                        world_thumbnail.src_base64 = base64_image
                        world_thumbnail.visible = True
                    except Exception:
                        world_thumbnail.visible = False
                else:
                    world_thumbnail.visible = False

                world_name_text.value = world.name
                world_name_text.visible = True
                world_info_loading.visible = False
                world_info_error.visible = False
                state.page.update()

            except UnauthorizedException:
                world_info_loading.visible = False
                state.page.update()
                state.handle_session_expiry()
            except ApiException as e:
                world_info_loading.visible = False
                world_thumbnail.visible = False
                world_name_text.visible = False
                world_info_error.value = "ワールドが見つかりません" if e.status == 404 else f"エラー: {e.status}"
                world_info_error.visible = True
                state.page.update()
            except Exception as e:
                world_info_loading.visible = False
                world_thumbnail.visible = False
                world_name_text.visible = False
                world_info_error.value = f"エラー: {str(e)}"
                world_info_error.visible = True
                state.page.update()

        threading.Thread(target=run, daemon=True).start()

    # ハンドラ参照を widgets に格納（show_main_view から初期取得に使用）
    widgets["fetch_world_info"] = fetch_world_info

    def on_world_id_change(e):
        state.debounce("world", 1.0, fetch_world_info, e.control.value.strip())

    def build_world_dropdown_item(world) -> ft.Container:
        return ft.Container(
            content=ft.Column([
                ft.Text(world.name, size=14, weight=ft.FontWeight.BOLD, color="white"),
                ft.Text(world.id, size=11, color="#888888"),
            ], spacing=2),
            padding=ft.padding.symmetric(vertical=6, horizontal=10),
            border_radius=4,
            bgcolor="#252535",
            on_click=lambda e, w=world: on_world_select(w),
            ink=True,
        )

    def on_world_search_change(e):
        query = e.control.value.strip().lower()
        if not query:
            world_dropdown.visible = False
            state.page.update()
            return
        filtered = [
            w for w in state.favorite_worlds
            if query in w.name.lower() or query in w.id.lower()
        ]
        world_dropdown.controls.clear()
        for w in filtered[:20]:
            world_dropdown.controls.append(build_world_dropdown_item(w))
        world_dropdown.visible = len(filtered) > 0
        state.page.update()

    def on_world_search_focus(e):
        if state.favorite_worlds and not world_search_field.value:
            world_dropdown.controls.clear()
            for w in state.favorite_worlds[:20]:
                world_dropdown.controls.append(build_world_dropdown_item(w))
            world_dropdown.visible = True
            state.page.update()

    def on_world_search_blur(e):
        def _hide():
            time.sleep(0.2)
            world_dropdown.visible = False
            state.page.update()
        threading.Thread(target=_hide, daemon=True).start()

    def on_world_select(world):
        world_search_field.value = world.name
        world_dropdown.visible = False
        world_id_field.value = world.id
        fetch_world_info(world.id)
        state.page.update()

    def on_create_click(e):
        def run():
            sys.stdout = state.gui_output
            try:
                state.set_buttons_disabled(True)
                if state.api_client is None:
                    state.log_queue.put("エラー: ログインしていません。\n")
                    return
                location = create_instance(
                    state.api_client,
                    world_id=world_id_field.value or None,
                    instance_type=instance_type_dropdown.value or None,
                    instance_region=instance_region_dropdown.value or None,
                )
                location_field.value = location
                location_row.visible = True
                state.page.update()
                if on_instance_created:
                    on_instance_created(location)
            except UnauthorizedException:
                state.handle_session_expiry()
            except Exception as ex:
                state.log_queue.put(f"エラー: {ex}\n")
            finally:
                sys.stdout = state.original_stdout
                state.set_buttons_disabled(False)

        threading.Thread(target=run, daemon=True).start()

    world_id_field.on_change = on_world_id_change
    world_search_field.on_change = on_world_search_change
    world_search_field.on_focus = on_world_search_focus
    world_search_field.on_blur = on_world_search_blur
    create_button.on_click = on_create_click


def build_instance_card(widgets: dict) -> ft.Card:
    """インスタンス作成セクションの Card レイアウトを組み立てて返す。"""
    return _styled_card(
        ft.Column([
            _section_header("dns", "インスタンスを作成"),
            ft.Divider(height=1, color="#333344"),
            ft.Row([
                ft.Column([
                    widgets["world_search_field"],
                    widgets["world_dropdown"],
                    widgets["world_id_field"],
                    ft.Row([widgets["instance_type_dropdown"], widgets["instance_region_dropdown"]], spacing=10),
                    ft.Container(height=10),
                    widgets["create_button"],
                    widgets["location_row"],
                ], expand=True, spacing=10),
                ft.Container(
                    content=ft.Column([
                        widgets["world_info_loading"],
                        widgets["world_thumbnail"],
                        widgets["world_name_text"],
                        widgets["world_info_error"],
                    ], horizontal_alignment=ft.CrossAxisAlignment.CENTER),
                    width=220,
                    padding=ft.padding.only(left=10),
                ),
            ]),
        ]),
    )
