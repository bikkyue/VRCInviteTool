"""
VRChat 認証の共通処理。
セッションCookieの保存・読み込み・削除はコールバック経由で外部から注入する。
"""

import json
import re
from http.cookiejar import CookieJar
from typing import Optional, Callable

import vrchatapi
from vrchatapi.api import authentication_api
from vrchatapi.exceptions import UnauthorizedException, ApiException
from vrchatapi.models.two_factor_auth_code import TwoFactorAuthCode
from vrchatapi.models.two_factor_email_code import TwoFactorEmailCode

USER_AGENT = "VRCInviteTool/0.1.0 (github.com/Droplet-Collective/VRCInviteTool)"

# 2FA コード: 6 桁の数字、またはリカバリーコード (xxxx-xxxx 形式)
_TWO_FACTOR_CODE_RE = re.compile(r"^(\d{6}|[a-z0-9]{4}-[a-z0-9]{4})$", re.IGNORECASE)
MAX_TWO_FACTOR_ATTEMPTS = 3


class LoginError(Exception):
    """ユーザーに表示可能な短いメッセージを持つログイン失敗例外。"""


def format_api_error(e: ApiException) -> str:
    """ApiException から HTTP ヘッダ/ボディの全文を含まない短い説明を作る。"""
    message = None
    try:
        body = e.body
        if isinstance(body, bytes):
            body = body.decode("utf-8", errors="replace")
        if body:
            message = json.loads(body).get("error", {}).get("message")
    except (ValueError, AttributeError, TypeError):
        message = None
    if message:
        return f"({e.status}) {message}"
    return f"({e.status}) {e.reason}"


# --- セッション保存・読み込み (コールバック方式) ---

def _serialize_cookies(cookie_jar: CookieJar) -> str:
    """CookieJarの内容をJSON文字列にシリアライズする。"""
    cookies = [
        {
            "name": c.name,
            "value": c.value,
            "domain": c.domain,
            "path": c.path,
            "expires": c.expires,
        }
        for c in cookie_jar
    ]
    return json.dumps(cookies)


def _deserialize_cookies(cookie_jar: CookieJar, json_str: str) -> bool:
    """JSON文字列からCookieJarにCookieを復元する。期限切れは除外する。"""
    import time
    now = time.time()
    data = json.loads(json_str)
    valid = [c for c in data if c["expires"] is None or c["expires"] > now]

    if not valid:
        return False

    for c in valid:
        ck = _make_cookie(c["name"], c["value"], c["domain"], c["path"], c["expires"])
        cookie_jar.set_cookie(ck)
    return True


def _make_cookie(name, value, domain, path, expires):
    """http.cookiejar.Cookie オブジェクトを生成する。"""
    import http.cookiejar as cj
    return cj.Cookie(
        version=0,
        name=name,
        value=value,
        port=None,
        port_specified=False,
        domain=domain,
        domain_specified=bool(domain),
        domain_initial_dot=domain.startswith(".") if domain else False,
        path=path,
        path_specified=bool(path),
        secure=True,
        expires=expires,
        discard=expires is None,
        comment=None,
        comment_url=None,
        rest={},
    )


# --- ログイン処理 ---

def _do_login(
    api_client: vrchatapi.ApiClient,
    input_fn: Optional[Callable[[str], Optional[str]]] = None,
):
    """ユーザー名・パスワードでログインし、2FAを処理する。成功時は CurrentUser を返す。

    input_fn が None を返した場合 (キャンセル) や 2FA 失敗時は LoginError を送出する。
    """
    _input = input_fn or input
    auth_api = authentication_api.AuthenticationApi(api_client)
    try:
        current_user = auth_api.get_current_user()
        print(f"ログイン成功: {current_user.display_name}")
        _drop_password(api_client)
        return current_user
    except UnauthorizedException as e:
        if e.status != 200:
            print(f"認証エラー: {format_api_error(e)}")
            return False
        is_email = "Email 2 Factor Authentication" in str(e.reason)

    # 2FA が必要 (例外ハンドラの外で処理し、誤入力時は再入力させる)
    prompt = (
        "メールに届いた2FAコードを入力してください: "
        if is_email
        else "認証アプリの2FAコードを入力してください: "
    )
    # 形式エラー (空入力・誤打) は試行回数を消費しない。API に拒否された場合のみカウントする。
    # ループはキャンセル (None) または GUI 側のタイムアウトで抜ける。
    attempts = 0
    verified = False
    while attempts < MAX_TWO_FACTOR_ATTEMPTS:
        code = _input(prompt)
        if code is None:
            raise LoginError("ログインをキャンセルしました。")
        code = code.strip()
        if not _TWO_FACTOR_CODE_RE.match(code):
            print("2FAコードの形式が正しくありません (6桁の数字、またはリカバリーコード xxxx-xxxx)。")
            prompt = "2FAコードの形式が正しくありません。もう一度入力してください: "
            continue
        try:
            if is_email:
                result = auth_api.verify2_fa_email_code(
                    two_factor_email_code=TwoFactorEmailCode(code=code)
                )
            elif "-" in code:
                # リカバリーコードは TOTP とは別エンドポイント
                result = auth_api.verify_recovery_code(
                    two_factor_auth_code=TwoFactorAuthCode(code=code)
                )
            else:
                result = auth_api.verify2_fa(
                    two_factor_auth_code=TwoFactorAuthCode(code=code)
                )
            verified = bool(getattr(result, "verified", False))
        except ApiException as e:
            if e.status not in (400, 401):
                raise LoginError(f"2FAコードの確認に失敗しました {format_api_error(e)}") from e
            verified = False
        if verified:
            break
        attempts += 1
        print("2FAコードが正しくありません。")
        prompt = "2FAコードが正しくありません。もう一度入力してください: "
    if not verified:
        raise LoginError("2FAコードの確認に失敗しました。ログインをやり直してください。")

    try:
        current_user = auth_api.get_current_user()
    except UnauthorizedException as e:
        raise LoginError(f"ログインに失敗しました {format_api_error(e)}") from e
    print(f"ログイン成功: {current_user.display_name}")
    _drop_password(api_client)
    return current_user


def _drop_password(api_client: vrchatapi.ApiClient) -> None:
    """Cookie 取得後はパスワードを保持せず、Basic 認証ヘッダの再送を止める。"""
    api_client.configuration.username = None
    api_client.configuration.password = None


def login(
    api_client: vrchatapi.ApiClient,
    input_fn: Optional[Callable[[str], Optional[str]]] = None,
    save_session: Optional[Callable[[str], None]] = None,
    load_session: Optional[Callable[[], Optional[str]]] = None,
    clear_session: Optional[Callable[[], None]] = None,
):
    """
    セッションが保存済みならそれを再利用し、
    無効・未保存の場合はログインしてセッションを保存する。
    成功時は CurrentUser、失敗時は None を返す。

    load_session を渡さない場合 (明示ログイン) は、保存済みセッションを破棄してから
    入力された認証情報でログインする。
    """
    cookie_jar: CookieJar = api_client.rest_client.cookie_jar
    auth_api = authentication_api.AuthenticationApi(api_client)

    # 保存済みセッションを試みる
    if load_session:
        json_str = load_session()
        if json_str and _deserialize_cookies(cookie_jar, json_str):
            try:
                current_user = auth_api.get_current_user()
                print(f"セッション再利用: {current_user.display_name}")
                _drop_password(api_client)
                return current_user
            except (UnauthorizedException, ApiException):
                print("保存済みセッションが無効です。再ログインします。")
                if clear_session:
                    clear_session()
                cookie_jar.clear()
    else:
        # 入力した認証情報と別アカウントの Cookie でログインしないよう破棄する
        if clear_session:
            clear_session()
        cookie_jar.clear()

    # 新規ログイン
    current_user = _do_login(api_client, input_fn=input_fn)
    if not current_user:
        return None

    if save_session:
        save_session(_serialize_cookies(cookie_jar))
    return current_user


# --- ApiClient ファクトリ ---

def create_api_client(
    username: Optional[str] = None,
    password: Optional[str] = None,
) -> vrchatapi.ApiClient:
    """設定済みの ApiClient を生成する。セッション再利用時はパスワード不要。"""
    configuration = vrchatapi.Configuration()
    if username:
        configuration.username = username
    if password:
        configuration.password = password
    api_client = vrchatapi.ApiClient(configuration)
    api_client.user_agent = USER_AGENT
    return api_client


def try_session_login(
    api_client: vrchatapi.ApiClient,
    load_session: Optional[Callable[[], Optional[str]]] = None,
    clear_session: Optional[Callable[[], None]] = None,
):
    """セッション再利用を試み、成功時に CurrentUser を返す。失敗時は None。"""
    if not load_session:
        return None
    json_str = load_session()
    if not json_str:
        return None
    cookie_jar: CookieJar = api_client.rest_client.cookie_jar
    if not _deserialize_cookies(cookie_jar, json_str):
        return None
    try:
        auth_api = authentication_api.AuthenticationApi(api_client)
        current_user = auth_api.get_current_user()
        return current_user
    except (UnauthorizedException, ApiException):
        if clear_session:
            clear_session()
        cookie_jar.clear()
        return None


def logout(
    api_client: Optional[vrchatapi.ApiClient] = None,
    clear_session: Optional[Callable[[], None]] = None,
) -> None:
    """サーバ側セッションを無効化し、保存済みセッションを破棄する。"""
    if api_client is not None:
        try:
            authentication_api.AuthenticationApi(api_client).logout()
        except Exception as e:
            print(f"サーバ側ログアウトに失敗: {e}")
        api_client.rest_client.cookie_jar.clear()
    if clear_session:
        clear_session()
