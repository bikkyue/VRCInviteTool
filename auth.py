"""
VRChat 認証の共通処理。
セッションCookieの保存・読み込み・削除はコールバック経由で外部から注入する。
"""

import json
from http.cookiejar import CookieJar
from typing import Optional, Callable

import vrchatapi
from vrchatapi.api import authentication_api
from vrchatapi.exceptions import UnauthorizedException, ApiException
from vrchatapi.models.two_factor_auth_code import TwoFactorAuthCode
from vrchatapi.models.two_factor_email_code import TwoFactorEmailCode

USER_AGENT = "VRCInviteTool/0.1.0 (github.com/bikkyue/VRC-VRCInviteTool)"


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
    input_fn: Optional[Callable[[str], str]] = None,
) -> bool:
    """ユーザー名・パスワードでログインし、2FAを処理する。"""
    _input = input_fn or input
    auth_api = authentication_api.AuthenticationApi(api_client)
    try:
        current_user = auth_api.get_current_user()
        print(f"ログイン成功: {current_user.display_name}")
        return True
    except UnauthorizedException as e:
        if e.status == 200:
            if "Email 2 Factor Authentication" in str(e.reason):
                code = _input("メールに届いた2FAコードを入力してください: ").strip()
                auth_api.verify2_fa_email_code(
                    two_factor_email_code=TwoFactorEmailCode(code=code)
                )
            else:
                code = _input("認証アプリの2FAコードを入力してください: ").strip()
                auth_api.verify2_fa(
                    two_factor_auth_code=TwoFactorAuthCode(code=code)
                )
            current_user = auth_api.get_current_user()
            print(f"ログイン成功: {current_user.display_name}")
            return True
        else:
            print(f"認証エラー: {e}")
            return False


def login(
    api_client: vrchatapi.ApiClient,
    input_fn: Optional[Callable[[str], str]] = None,
    save_session: Optional[Callable[[str], None]] = None,
    load_session: Optional[Callable[[], Optional[str]]] = None,
    clear_session: Optional[Callable[[], None]] = None,
) -> bool:
    """
    セッションが保存済みならそれを再利用し、
    無効・未保存の場合はログインしてセッションを保存する。
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
                return True
            except (UnauthorizedException, ApiException):
                print("保存済みセッションが無効です。再ログインします。")
                if clear_session:
                    clear_session()
                cookie_jar.clear()

    # 新規ログイン
    if not _do_login(api_client, input_fn=input_fn):
        return False

    if save_session:
        save_session(_serialize_cookies(cookie_jar))
    return True


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
    configuration.client_side_validation = False
    vrchatapi.Configuration.set_default(configuration)
    api_client = vrchatapi.ApiClient(configuration)
    api_client.user_agent = USER_AGENT
    return api_client


def try_session_login(
    api_client: vrchatapi.ApiClient,
    load_session: Optional[Callable[[], Optional[str]]] = None,
    clear_session: Optional[Callable[[], None]] = None,
) -> Optional[str]:
    """セッション再利用を試み、成功時に display_name を返す。失敗時は None。"""
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
        return current_user.display_name
    except (UnauthorizedException, ApiException):
        if clear_session:
            clear_session()
        cookie_jar.clear()
        return None


def logout(
    clear_session: Optional[Callable[[], None]] = None,
) -> None:
    """保存済みセッションを破棄する。"""
    if clear_session:
        clear_session()
