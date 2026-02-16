"""AppState クラス — アプリ全体の共有状態を保持する。"""

import json
import os
import sys
import time
import queue
import threading
from pathlib import Path

import flet as ft


# --- ファイルベース設定ストレージ ---

def _get_config_path() -> Path:
    """設定ファイルのパスを返す。"""
    appdata = os.environ.get("APPDATA")
    if appdata:
        config_dir = Path(appdata) / "VRCInviteTool"
    else:
        config_dir = Path.home() / ".vrcinvitetool"
    config_dir.mkdir(parents=True, exist_ok=True)
    return config_dir / "config.json"


def _load_config() -> dict:
    path = _get_config_path()
    if path.exists():
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}
    return {}


def _save_config(data: dict):
    path = _get_config_path()
    path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")


class GUIOutput:
    """sys.stdout をキューに差し替えるラッパー。"""

    def __init__(self, q: queue.Queue):
        self._q = q

    def write(self, text: str):
        if text:
            self._q.put(text)

    def flush(self):
        pass


class AppState:
    """アプリ全体の共有状態を保持するクラス。"""

    def __init__(self, page: ft.Page):
        self.page = page

        # VRChat API 状態
        self.api_client = None
        self.display_name = ""
        self.user_id = ""
        self.friends: list = []
        self.favorite_worlds: list = []

        # ログ
        self.log_queue: queue.Queue = queue.Queue()
        self.log_lock = threading.Lock()
        self.original_stdout = sys.stdout
        self.gui_output = GUIOutput(self.log_queue)

        # デバウンス
        self._debounce_timers: dict = {"world": None, "user": None}
        self._button_unlock_time = 0.0

        # ボタン一覧（セクション側から登録）
        self._action_buttons: list = []

        # ログフィールド（log_section 側から登録）
        self._log_field = None

        # ログイン画面表示コールバック（通常のログアウト用）
        self._show_login_fn = None

        # セッション切れ時に追加でエラー表示するコールバック
        self._session_expired_message_fn = None

    # --- ログフィールド登録 ---

    def set_log_field(self, field: ft.TextField):
        self._log_field = field

    # --- ボタン管理 ---

    def register_action_button(self, btn):
        self._action_buttons.append(btn)

    # --- 設定ファイルストレージ ---

    def save_session(self, json_str: str):
        config = _load_config()
        config["session_cookies"] = json_str
        _save_config(config)

    def load_session(self):
        return _load_config().get("session_cookies")

    def clear_session(self):
        config = _load_config()
        config.pop("session_cookies", None)
        _save_config(config)

    def save_username(self, username: str):
        config = _load_config()
        config["username"] = username
        _save_config(config)

    def load_username(self) -> str:
        return _load_config().get("username", "")

    # --- ユーティリティ ---

    def append_log(self, text: str):
        if self._log_field is None:
            return
        with self.log_lock:
            self._log_field.value = (self._log_field.value or "") + text
        self.page.update()

    def set_buttons_disabled(self, disabled: bool):
        if disabled:
            self._button_unlock_time = time.time() + 2.0
            for btn in self._action_buttons:
                btn.disabled = True
            self.page.update()
        else:
            remaining = self._button_unlock_time - time.time()
            if remaining > 0:
                def delayed_enable():
                    time.sleep(remaining)
                    for btn in self._action_buttons:
                        btn.disabled = False
                    self.page.update()
                threading.Thread(target=delayed_enable, daemon=True).start()
            else:
                for btn in self._action_buttons:
                    btn.disabled = False
                self.page.update()

    def debounce(self, key: str, delay: float, fn, *args):
        if self._debounce_timers.get(key):
            self._debounce_timers[key].cancel()
        t = threading.Timer(delay, fn, args=args)
        t.start()
        self._debounce_timers[key] = t

    # --- 2FA ダイアログ ---

    def two_factor_input_fn(self, prompt: str) -> str:
        from .theme import COLOR_PRIMARY, COLOR_ACCENT
        result = {"value": ""}
        evt = threading.Event()
        code_field = ft.TextField(
            label="2FAコード",
            autofocus=True,
            border_color=COLOR_PRIMARY,
            focused_border_color=COLOR_ACCENT,
        )

        def on_ok(e):
            result["value"] = code_field.value or ""
            self.page.close(dlg)
            evt.set()

        code_field.on_submit = on_ok

        dlg = ft.AlertDialog(
            modal=True,
            title=ft.Text("二段階認証"),
            content=ft.Column(
                controls=[ft.Text(prompt), code_field],
                tight=True,
            ),
            actions=[ft.TextButton("OK", on_click=on_ok)],
        )

        self.page.open(dlg)
        self.page.update()
        evt.wait()
        return result["value"]

    # --- セッション切れ処理 ---

    def handle_session_expiry(self):
        from auth import logout
        logout(clear_session=self.clear_session)
        self.api_client = None
        self.display_name = ""
        if self._show_login_fn:
            self._show_login_fn()
        if self._session_expired_message_fn:
            self._session_expired_message_fn()

    # --- ログフラッシュスレッド ---

    def start_log_flush(self):
        def log_flush_loop():
            while True:
                chunks = []
                try:
                    while True:
                        chunks.append(self.log_queue.get_nowait())
                except queue.Empty:
                    pass
                if chunks:
                    self.append_log("".join(chunks))
                time.sleep(0.1)

        threading.Thread(target=log_flush_loop, daemon=True).start()
