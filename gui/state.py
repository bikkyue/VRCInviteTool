"""AppState クラス — アプリ全体の共有状態を保持する。"""

import json
import os
import sys
import time
import queue
import tempfile
import threading
from pathlib import Path

import flet as ft

# 2FA ダイアログの応答待ちタイムアウト (秒)
TWO_FACTOR_DIALOG_TIMEOUT_SEC = 300


# --- ファイルベース設定ストレージ ---

def _get_config_path() -> Path:
    """設定ファイルのパスを返す。"""
    appdata = os.environ.get("APPDATA")
    if appdata:
        config_dir = Path(appdata) / "VRCInviteTool"
    else:
        config_dir = Path.home() / ".vrcinvitetool"
    config_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    return config_dir / "config.json"


def _load_config() -> dict:
    path = _get_config_path()
    if path.exists():
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}
    return {}


_config_lock = threading.Lock()


def _save_config(data: dict):
    """一時ファイルに書いてから置き換える (クラッシュ時の破損防止)。

    並行呼び出し (起動時 auto-login と手動ログイン等) に備えてロックで直列化し、
    一時ファイルは一意名・0o600 で作成する。
    """
    path = _get_config_path()
    with _config_lock:
        fd, tmp_name = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".", suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                f.write(json.dumps(data, ensure_ascii=False))
            # Windows では対象が他プロセスに開かれていると PermissionError になるため短時間リトライ
            for retry in range(3):
                try:
                    os.replace(tmp_name, path)
                    break
                except PermissionError:
                    if retry == 2:
                        raise
                    time.sleep(0.05)
        except BaseException:
            try:
                os.unlink(tmp_name)
            except OSError:
                pass
            raise


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

    def two_factor_input_fn(self, prompt: str):
        """2FA コード入力ダイアログを表示する。キャンセル/タイムアウト時は None を返す。"""
        from .theme import COLOR_PRIMARY, COLOR_ACCENT
        result = {"value": None}
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

        def on_cancel(e):
            result["value"] = None
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
            actions=[
                ft.TextButton("キャンセル", on_click=on_cancel),
                ft.TextButton("OK", on_click=on_ok),
            ],
        )

        self.page.open(dlg)
        self.page.update()
        if not evt.wait(timeout=TWO_FACTOR_DIALOG_TIMEOUT_SEC):
            self.page.close(dlg)
            self.page.update()
            return None
        return result["value"]

    # --- セッション切れ処理 ---

    def handle_session_expiry(self):
        from auth import logout
        # セッション切れ時はサーバ側 logout は呼ばず、ローカルのみ破棄する
        logout(clear_session=self.clear_session)
        self.api_client = None
        self.display_name = ""
        self.user_id = ""
        self.friends = []
        self.favorite_worlds = []
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
