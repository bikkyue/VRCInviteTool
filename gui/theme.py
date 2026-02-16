"""カラー定数と共通 UI ヘルパー関数。"""

import flet as ft

# --- カラーパレット ---
COLOR_PRIMARY = "#1E88E5"       # VRChat風ブルー
COLOR_PRIMARY_DARK = "#1565C0"
COLOR_ACCENT = "#26C6DA"        # シアンアクセント
COLOR_SUCCESS = "#66BB6A"       # 緑（保存ボタン）
COLOR_INVITE = "#AB47BC"        # パープル（招待ボタン）
COLOR_CARD_BG = "#1E1E2E"       # ダークカード背景
COLOR_PAGE_BG = "#121218"       # ページ背景
COLOR_HEADER_TEXT = "#BBDEFB"   # 薄いブルー（ヘッダー文字）

# --- ボタン無効時の色 ---
_DISABLED_BGCOLOR = "#3A3A4A"
_DISABLED_COLOR = "#666677"


def _section_header(icon: str, title: str) -> ft.Row:
    """アイコン付きセクションヘッダーを作成する。"""
    return ft.Row(
        controls=[
            ft.Icon(name=icon, color=COLOR_ACCENT, size=22),
            ft.Text(
                title,
                size=17,
                weight=ft.FontWeight.BOLD,
                color=COLOR_HEADER_TEXT,
            ),
        ],
        spacing=8,
    )


def _styled_card(content: ft.Control) -> ft.Card:
    """統一スタイルのカードを作成する。"""
    return ft.Card(
        content=ft.Container(
            content=content,
            padding=20,
            bgcolor=COLOR_CARD_BG,
            border_radius=12,
        ),
        elevation=4,
        color=COLOR_CARD_BG,
    )


def _title_banner(right_content: ft.Control = None) -> ft.Container:
    """タイトルバナーを作成する。right_content があれば右端に配置する。"""
    left = ft.Row(
        controls=[
            ft.Icon(name="rocket_launch", color=COLOR_ACCENT, size=28),
            ft.Text(
                "VRCInviteTool",
                size=24,
                weight=ft.FontWeight.BOLD,
                color="white",
            ),
        ],
        spacing=10,
    )
    row_controls = [left]
    if right_content:
        row_controls.append(right_content)

    return ft.Container(
        content=ft.Row(
            controls=row_controls,
            alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
            vertical_alignment=ft.CrossAxisAlignment.CENTER,
        ),
        padding=ft.padding.symmetric(vertical=12, horizontal=16),
        border_radius=12,
        gradient=ft.LinearGradient(
            begin=ft.alignment.center_left,
            end=ft.alignment.center_right,
            colors=[COLOR_PRIMARY_DARK, COLOR_PRIMARY, COLOR_ACCENT],
        ),
        margin=ft.margin.only(bottom=8),
    )
