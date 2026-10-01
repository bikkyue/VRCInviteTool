//! VRChat API クライアント本体。
//!
//! * すべてのリクエストに接続 10 秒 / 全体 30 秒のタイムアウト
//! * 429 は `Retry-After` を見て最大 3 回リトライ
//! * 401 は [`Error::Unauthorized`] として返し、UI 側でログイン画面へ戻す
//! * Cookie (auth / twoFactorAuth) はクライアント内の jar に保持し、JSON で出し入れできる

use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

use cookie_store::CookieStore;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::header::RETRY_AFTER;
use reqwest::StatusCode;
use reqwest_cookie_store::CookieStoreMutex;
use serde::de::DeserializeOwned;
use url::Url;

use super::error::{Error, Result};
use super::models::*;
use super::retry::retry_delay;
use crate::validate;

/// 本番 API のベース URL (末尾スラッシュ必須)。
pub const DEFAULT_BASE_URL: &str = "https://api.vrchat.cloud/api/1/";
/// 接続タイムアウト。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// リクエスト全体のタイムアウト。
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// 一覧取得の 1 ページ件数 (API 上限は 100)。
pub const PAGE_SIZE: usize = 100;
/// 一覧取得の最大ページ数 (無限ループ防止)。
const MAX_PAGES: usize = 100;
/// 取得する画像の最大サイズ。
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Python の `urllib.parse.quote(s, safe='')` 相当: 英数字と `-_.~` 以外をエンコードする。
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// `GET /auth/user` の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthResponse {
    /// ログイン完了 (auth + twoFactorAuth Cookie が有効)。
    LoggedIn(CurrentUser),
    /// 2FA が必要。`methods` は `requiresTwoFactorAuth` の値 (`["emailOtp"]` または `["totp","otp"]`)。
    TwoFactorRequired { methods: Vec<String> },
}

/// VRChat API クライアント。`Clone` は内部の HTTP クライアントと Cookie jar を共有する。
#[derive(Clone)]
pub struct VrcClient {
    http: reqwest::Client,
    jar: Arc<CookieStoreMutex>,
    base: Url,
}

impl std::fmt::Debug for VrcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VrcClient")
            .field("base", &self.base.as_str())
            .finish()
    }
}

impl VrcClient {
    /// 本番 API に向けたクライアントを作る。
    pub fn new(user_agent: &str) -> Result<Self> {
        Self::with_base_url(DEFAULT_BASE_URL, user_agent)
    }

    /// ベース URL を指定してクライアントを作る (テスト用)。
    pub fn with_base_url(base_url: &str, user_agent: &str) -> Result<Self> {
        let mut base = Url::parse(base_url)
            .map_err(|e| Error::Validation(format!("ベースURLが不正です: {e}")))?;
        if !base.path().ends_with('/') {
            let p = format!("{}/", base.path());
            base.set_path(&p);
        }
        let jar = Arc::new(CookieStoreMutex::new(CookieStore::new()));
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .cookie_provider(jar.clone())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| Error::Network(format!("HTTPクライアントの初期化に失敗しました: {e}")))?;
        Ok(Self { http, jar, base })
    }

    // ------------------------------------------------------------------
    // Cookie jar
    // ------------------------------------------------------------------

    /// jar 内の Cookie をすべて削除する。
    pub fn clear_cookies(&self) {
        if let Ok(mut store) = self.jar.lock() {
            store.clear();
        }
    }

    /// 期限内の Cookie を JSON にシリアライズする (永続化用)。
    pub fn export_cookies(&self) -> Result<String> {
        let store = self
            .jar
            .lock()
            .map_err(|_| Error::Session("cookie jar lock poisoned".into()))?;
        let mut buf = Vec::new();
        cookie_store::serde::json::save(&store, &mut buf)
            .map_err(|e| Error::Session(format!("Cookieのシリアライズに失敗しました: {e}")))?;
        String::from_utf8(buf).map_err(|e| Error::Session(e.to_string()))
    }

    /// JSON から Cookie を復元する。期限切れの Cookie は読み込み時に除外される。
    /// 既存の Cookie は置き換えられる。
    pub fn import_cookies(&self, json: &str) -> Result<()> {
        let loaded = cookie_store::serde::json::load(BufReader::new(json.as_bytes()))
            .map_err(|e| Error::Session(format!("Cookieの読み込みに失敗しました: {e}")))?;
        let mut store = self
            .jar
            .lock()
            .map_err(|_| Error::Session("cookie jar lock poisoned".into()))?;
        *store = loaded;
        Ok(())
    }

    /// 指定名の期限内 Cookie が jar にあるか。
    pub fn has_cookie(&self, name: &str) -> bool {
        self.jar
            .lock()
            .map(|store| store.iter_unexpired().any(|c| c.name() == name))
            .unwrap_or(false)
    }

    /// 期限内 Cookie の名前一覧 (ログ・診断用。値は返さない)。
    pub fn cookie_names(&self) -> Vec<String> {
        self.jar
            .lock()
            .map(|store| {
                store
                    .iter_unexpired()
                    .map(|c| c.name().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // 低レベル
    // ------------------------------------------------------------------

    fn url(&self, path: &str) -> Url {
        // path はこのモジュール内で組み立てる固定文字列なので join は失敗しない
        self.base.join(path).expect("valid relative API path")
    }

    fn encode_segment(s: &str) -> String {
        utf8_percent_encode(s, PATH_SEGMENT).to_string()
    }

    /// `wrld_x:inst` 形式のパス片。instanceId 側だけをエンコードする。
    fn location_segment(world_id: &str, instance_id: &str) -> String {
        format!(
            "{}:{}",
            Self::encode_segment(world_id),
            Self::encode_segment(instance_id)
        )
    }

    /// リクエストを送る。429 は `Retry-After` に従ってリトライする。
    async fn execute(&self, req: reqwest::Request) -> Result<reqwest::Response> {
        let mut attempt = 0u32;
        loop {
            let this = req
                .try_clone()
                .ok_or_else(|| Error::Network("リクエストを複製できません".into()))?;
            let resp = self.http.execute(this).await?;
            if resp.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_after = resp
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                if let Some(delay) = retry_delay(attempt, retry_after.as_deref()) {
                    tracing::warn!(
                        path = req.url().path(),
                        attempt,
                        delay_secs = delay.as_secs(),
                        "429 Too Many Requests: リトライします"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
            }
            return Ok(resp);
        }
    }

    /// レスポンスを検査し、成功なら本文を返す。失敗ステータスは [`Error`] に変換する。
    async fn success_body(resp: reqwest::Response) -> Result<String> {
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(Error::from_response(status.as_u16(), &body))
        }
    }

    async fn send_json<T: DeserializeOwned>(&self, rb: reqwest::RequestBuilder) -> Result<T> {
        let req = rb.build()?;
        let resp = self.execute(req).await?;
        let body = Self::success_body(resp).await?;
        Ok(serde_json::from_str(&body)?)
    }

    // ------------------------------------------------------------------
    // 認証
    // ------------------------------------------------------------------

    /// `GET /auth/user`。`basic` を渡すと Basic 認証でログインする (ユーザー名・パスワードは
    /// VRChat の仕様に合わせて URL エンコードしてから Base64 化する)。渡さなければ Cookie で認証する。
    pub async fn get_current_user(&self, basic: Option<(&str, &str)>) -> Result<AuthResponse> {
        let mut rb = self.http.get(self.url("auth/user"));
        if let Some((user, pass)) = basic {
            rb = rb.basic_auth(
                utf8_percent_encode(user, PATH_SEGMENT).to_string(),
                Some(utf8_percent_encode(pass, PATH_SEGMENT).to_string()),
            );
        }
        let body: serde_json::Value = self.send_json(rb).await?;
        Self::parse_auth_response(body)
    }

    /// `GET /auth/user` の 200 応答を解釈する。
    /// 2FA が必要な場合は `{"requiresTwoFactorAuth":["emailOtp"]}` のように返ってくる。
    pub fn parse_auth_response(body: serde_json::Value) -> Result<AuthResponse> {
        if let Some(arr) = body
            .get("requiresTwoFactorAuth")
            .and_then(|v| v.as_array())
            .filter(|arr| !arr.is_empty())
        {
            let methods = arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            return Ok(AuthResponse::TwoFactorRequired { methods });
        }
        let user: CurrentUser = serde_json::from_value(body)?;
        Ok(AuthResponse::LoggedIn(user))
    }

    /// `POST /auth/twofactorauth/{emailotp|totp|otp}/verify`。
    /// コードが拒否された場合 (400/401) は `Ok(false)`。
    pub async fn verify_two_factor(&self, kind: TwoFactorKind, code: &str) -> Result<bool> {
        let path = format!("auth/twofactorauth/{}/verify", kind.path_segment());
        let req = self
            .http
            .post(self.url(&path))
            .json(&TwoFactorVerifyRequest {
                code: code.trim().to_string(),
            })
            .build()?;
        let resp = self.execute(req).await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() {
            let parsed: TwoFactorVerifyResponse = serde_json::from_str(&body)?;
            return Ok(parsed.verified);
        }
        if status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED {
            tracing::info!(status = status.as_u16(), "2FAコードが拒否されました");
            return Ok(false);
        }
        Err(Error::from_response(status.as_u16(), &body))
    }

    /// `PUT /logout`。
    pub async fn logout(&self) -> Result<()> {
        let req = self.http.put(self.url("logout")).build()?;
        let resp = self.execute(req).await?;
        Self::success_body(resp).await.map(|_| ())
    }

    // ------------------------------------------------------------------
    // ユーザー / フレンド
    // ------------------------------------------------------------------

    /// 全フレンド (オンライン + オフライン) を取得する。
    pub async fn list_friends(&self) -> Result<Vec<Friend>> {
        let mut all = Vec::new();
        for offline in [false, true] {
            let mut offset = 0usize;
            for _ in 0..MAX_PAGES {
                let rb = self.http.get(self.url("auth/user/friends")).query(&[
                    ("offline", offline.to_string()),
                    ("n", PAGE_SIZE.to_string()),
                    ("offset", offset.to_string()),
                ]);
                let batch: Vec<Friend> = self.send_json(rb).await?;
                let n = batch.len();
                all.extend(batch);
                if n < PAGE_SIZE {
                    break;
                }
                offset += n;
            }
        }
        Ok(all)
    }

    /// `GET /users/{id}`。
    pub async fn get_user(&self, user_id: &str) -> Result<User> {
        let id = validate::validate_user_id(user_id)?;
        let rb = self
            .http
            .get(self.url(&format!("users/{}", Self::encode_segment(id))));
        self.send_json(rb).await
    }

    // ------------------------------------------------------------------
    // ワールド / インスタンス
    // ------------------------------------------------------------------

    /// `GET /worlds/{id}`。
    pub async fn get_world(&self, world_id: &str) -> Result<World> {
        let id = validate::validate_world_id(world_id)?;
        let rb = self
            .http
            .get(self.url(&format!("worlds/{}", Self::encode_segment(id))));
        self.send_json(rb).await
    }

    /// お気に入りワールドを全件取得する (`GET /worlds/favorites`)。
    pub async fn list_favorite_worlds(&self) -> Result<Vec<FavoritedWorld>> {
        let mut all = Vec::new();
        let mut offset = 0usize;
        for _ in 0..MAX_PAGES {
            let rb = self
                .http
                .get(self.url("worlds/favorites"))
                .query(&[("n", PAGE_SIZE.to_string()), ("offset", offset.to_string())]);
            let batch: Vec<FavoritedWorld> = self.send_json(rb).await?;
            let n = batch.len();
            all.extend(batch);
            if n < PAGE_SIZE {
                break;
            }
            offset += n;
        }
        Ok(all)
    }

    /// 自分が作成したワールドを全件取得する (`GET /worlds?user=me&releaseStatus=all`)。
    /// 非公開 (private) のワールドも含む (自分ならインスタンスを作れるため)。
    pub async fn list_own_worlds(&self) -> Result<Vec<OwnWorld>> {
        let mut all = Vec::new();
        let mut offset = 0usize;
        for _ in 0..MAX_PAGES {
            let rb = self.http.get(self.url("worlds")).query(&[
                ("user", "me".to_string()),
                ("releaseStatus", "all".to_string()),
                ("n", PAGE_SIZE.to_string()),
                ("offset", offset.to_string()),
            ]);
            let batch: Vec<OwnWorld> = self.send_json(rb).await?;
            let n = batch.len();
            all.extend(batch);
            if n < PAGE_SIZE {
                break;
            }
            offset += n;
        }
        Ok(all)
    }

    /// `POST /instances`。
    pub async fn create_instance(&self, req: &CreateInstanceRequest) -> Result<Instance> {
        validate::validate_world_id(&req.world_id)?;
        validate::validate_region(&req.region)?;
        let rb = self.http.post(self.url("instances")).json(req);
        self.send_json(rb).await
    }

    /// `GET /instances/{worldId}:{instanceId}`。
    pub async fn get_instance(&self, location: &str) -> Result<Instance> {
        let (world_id, instance_id) = validate::split_location(location)?;
        let path = format!(
            "instances/{}",
            Self::location_segment(world_id, instance_id)
        );
        let rb = self.http.get(self.url(&path));
        self.send_json(rb).await
    }

    // ------------------------------------------------------------------
    // 招待
    // ------------------------------------------------------------------

    /// `POST /invite/{userId}`。招待先はフレンドである必要がある (そうでなければ 403)。
    pub async fn invite_user(&self, user_id: &str, location: &str) -> Result<Notification> {
        let id = validate::validate_user_id(user_id)?;
        validate::split_location(location)?;
        let rb = self
            .http
            .post(self.url(&format!("invite/{}", Self::encode_segment(id))))
            .json(&InviteRequest {
                instance_id: location.trim().to_string(),
                message_slot: None,
            });
        self.send_json(rb).await
    }

    /// `POST /invite/myself/to/{worldId}:{instanceId}`。
    pub async fn invite_myself(&self, location: &str) -> Result<Notification> {
        let (world_id, instance_id) = validate::split_location(location)?;
        let path = format!(
            "invite/myself/to/{}",
            Self::location_segment(world_id, instance_id)
        );
        let rb = self.http.post(self.url(&path));
        self.send_json(rb).await
    }

    // ------------------------------------------------------------------
    // 画像
    // ------------------------------------------------------------------

    /// サムネイル画像を取得して `data:` URL にして返す。
    /// `*.vrchat.cloud` 以外のホストは拒否する (Cookie はいずれにせよドメイン一致時のみ送られる)。
    pub async fn fetch_image_data_url(&self, image_url: &str) -> Result<String> {
        let url = Url::parse(image_url)
            .map_err(|e| Error::Validation(format!("画像URLが不正です: {e}")))?;
        if !Self::is_allowed_image_host(&url) {
            return Err(Error::Validation(format!(
                "許可されていない画像URLです: {}",
                url.host_str().unwrap_or("")
            )));
        }
        let req = self.http.get(url).build()?;
        let resp = self.execute(req).await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::from_response(status.as_u16(), &body));
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or("").trim().to_string())
            .filter(|s| s.starts_with("image/"))
            .ok_or_else(|| Error::Decode("画像ではないレスポンスが返されました".into()))?;
        if resp
            .content_length()
            .is_some_and(|len| len > MAX_IMAGE_BYTES as u64)
        {
            return Err(Error::Decode("画像が大きすぎます".into()));
        }
        let bytes = resp.bytes().await?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(Error::Decode("画像が大きすぎます".into()));
        }
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(format!("data:{mime};base64,{b64}"))
    }

    /// 画像取得を許可するホストか (`https` の `vrchat.cloud` 配下のみ)。
    pub fn is_allowed_image_host(url: &Url) -> bool {
        if url.scheme() != "https" {
            return false;
        }
        match url.host_str() {
            Some(h) => h == "vrchat.cloud" || h.ends_with(".vrchat.cloud"),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> VrcClient {
        VrcClient::with_base_url("https://api.example.test/api/1", "UA/1").unwrap()
    }

    #[test]
    fn base_url_gets_trailing_slash() {
        let c = client();
        assert_eq!(
            c.url("auth/user").as_str(),
            "https://api.example.test/api/1/auth/user"
        );
    }

    #[test]
    fn location_segment_encodes_instance_id_only() {
        let seg = VrcClient::location_segment("wrld_a", "12345~region(jp)~private(usr_b)");
        assert_eq!(seg, "wrld_a:12345~region%28jp%29~private%28usr_b%29");
        let c = client();
        assert_eq!(
            c.url(&format!("instances/{seg}")).as_str(),
            "https://api.example.test/api/1/instances/wrld_a:12345~region%28jp%29~private%28usr_b%29"
        );
    }

    #[test]
    fn parses_two_factor_required() {
        let r = VrcClient::parse_auth_response(
            serde_json::json!({"requiresTwoFactorAuth": ["emailOtp"]}),
        )
        .unwrap();
        assert_eq!(
            r,
            AuthResponse::TwoFactorRequired {
                methods: vec!["emailOtp".into()]
            }
        );
        let r = VrcClient::parse_auth_response(
            serde_json::json!({"requiresTwoFactorAuth": ["totp", "otp"]}),
        )
        .unwrap();
        assert_eq!(
            r,
            AuthResponse::TwoFactorRequired {
                methods: vec!["totp".into(), "otp".into()]
            }
        );
    }

    #[test]
    fn parses_current_user() {
        let r = VrcClient::parse_auth_response(serde_json::json!({
            "id": "usr_1", "displayName": "Alice", "username": "alice", "extra": 1
        }))
        .unwrap();
        match r {
            AuthResponse::LoggedIn(u) => {
                assert_eq!(u.id, "usr_1");
                assert_eq!(u.display_name, "Alice");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(VrcClient::parse_auth_response(serde_json::json!({"nope": true})).is_err());
        // 空の requiresTwoFactorAuth はログイン済みとして扱う
        let r = VrcClient::parse_auth_response(serde_json::json!({
            "id": "usr_1", "displayName": "Alice", "requiresTwoFactorAuth": []
        }))
        .unwrap();
        assert!(matches!(r, AuthResponse::LoggedIn(_)));
    }

    #[test]
    fn image_host_allowlist() {
        let ok = |s: &str| VrcClient::is_allowed_image_host(&Url::parse(s).unwrap());
        assert!(ok("https://api.vrchat.cloud/api/1/image/file_x/1/256"));
        assert!(ok("https://files.vrchat.cloud/thumbnails/x.png"));
        assert!(!ok("http://api.vrchat.cloud/x"));
        assert!(!ok("https://evil.example/api.vrchat.cloud"));
        assert!(!ok("https://notvrchat.cloud/x"));
    }

    #[test]
    fn cookie_export_import_round_trip() {
        let c = client();
        {
            let mut store = c.jar.lock().unwrap();
            let url = Url::parse("https://api.example.test/api/1/auth/user").unwrap();
            let far = "auth=authcookie; Path=/; Domain=api.example.test; Secure; HttpOnly; Max-Age=31536000";
            store.parse(far, &url).unwrap();
            let tfa = "twoFactorAuth=tfacookie; Path=/; Domain=api.example.test; Secure; HttpOnly; Max-Age=2592000";
            store.parse(tfa, &url).unwrap();
        }
        assert!(c.has_cookie("auth"));
        assert!(c.has_cookie("twoFactorAuth"));
        let json = c.export_cookies().unwrap();
        assert!(json.contains("authcookie"));

        let c2 = client();
        assert!(!c2.has_cookie("auth"));
        c2.import_cookies(&json).unwrap();
        assert!(c2.has_cookie("auth"));
        assert!(c2.has_cookie("twoFactorAuth"));
        let mut names = c2.cookie_names();
        names.sort();
        assert_eq!(names, vec!["auth", "twoFactorAuth"]);

        c2.clear_cookies();
        assert!(!c2.has_cookie("auth"));
        assert!(c2.import_cookies("not json").is_err());
    }

    #[test]
    fn expired_cookies_are_dropped_on_import() {
        let c = client();
        {
            let mut store = c.jar.lock().unwrap();
            let url = Url::parse("https://api.example.test/api/1/auth/user").unwrap();
            store
                .parse(
                    "auth=a; Path=/; Domain=api.example.test; Secure; Max-Age=31536000",
                    &url,
                )
                .unwrap();
            // twoFactorAuth だけ 1 秒で失効させる
            store
                .parse(
                    "twoFactorAuth=t; Path=/; Domain=api.example.test; Secure; Max-Age=1",
                    &url,
                )
                .unwrap();
        }
        let json = c.export_cookies().unwrap();
        assert!(json.contains("twoFactorAuth"));
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let c2 = client();
        c2.import_cookies(&json).unwrap();
        assert!(c2.has_cookie("auth"));
        assert!(!c2.has_cookie("twoFactorAuth"));
    }
}
