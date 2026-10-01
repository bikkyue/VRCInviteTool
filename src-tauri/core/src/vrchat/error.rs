//! クライアントのエラー型。ユーザーに見せる文言は `(status) message` 形式で、
//! HTTP ヘッダやボディ全文は含めない。

use std::fmt;

/// VRChat API / 通信 / 入力に関するエラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 401: セッション切れ・認証失敗。UI はログイン画面に戻す。
    Unauthorized { message: String },
    /// 401 以外の API エラー (`(status) message`)。
    Api { status: u16, message: String },
    /// タイムアウト・接続失敗など。
    Network(String),
    /// レスポンスが期待した JSON ではなかった。
    Decode(String),
    /// 入力値の形式が不正。
    Validation(String),
    /// セッション保存・読み込み (ファイル / DPAPI) の失敗。
    Session(String),
    /// 指定された 2FA コードの形式では送信先を決められない等、ログイン手順上のエラー。
    Login(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// フロントエンドに渡す種別文字列。
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Unauthorized { .. } => "unauthorized",
            Error::Api { .. } => "api",
            Error::Network(_) => "network",
            Error::Decode(_) => "decode",
            Error::Validation(_) => "validation",
            Error::Session(_) => "session",
            Error::Login(_) => "login",
        }
    }

    /// HTTP ステータス (API エラーのみ)。
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Unauthorized { .. } => Some(401),
            Error::Api { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Error::Unauthorized { .. })
    }

    /// HTTP ステータスと JSON ボディから API エラーを作る。
    /// ボディは `{"error":{"message":"...","status_code":401}}` 形式を想定する。
    pub fn from_response(status: u16, body: &str) -> Error {
        let message =
            extract_api_message(body).unwrap_or_else(|| default_reason(status).to_string());
        if status == 401 {
            Error::Unauthorized { message }
        } else {
            Error::Api { status, message }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unauthorized { message } => write!(f, "(401) {message}"),
            Error::Api { status, message } => write!(f, "({status}) {message}"),
            Error::Network(m) => write!(f, "通信エラー: {m}"),
            Error::Decode(m) => write!(f, "レスポンスの解析に失敗しました: {m}"),
            Error::Validation(m) => f.write_str(m),
            Error::Session(m) => write!(f, "セッションの保存/読み込みに失敗しました: {m}"),
            Error::Login(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // reqwest::Error の Display は URL を含むことがあるが、認証情報は含まれない。
        let msg = if e.is_timeout() {
            "タイムアウトしました".to_string()
        } else if e.is_connect() {
            "サーバーに接続できません".to_string()
        } else if e.is_decode() {
            return Error::Decode(e.to_string());
        } else {
            e.to_string()
        };
        Error::Network(msg)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Decode(e.to_string())
    }
}

/// `{"error":{"message":"..."}}` から message を取り出す。
fn extract_api_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let msg = v.get("error")?.get("message")?;
    match msg {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::String(_) => None,
        other => Some(other.to_string()),
    }
}

fn default_reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_uses_body_message() {
        let e = Error::from_response(
            403,
            r#"{"error":{"message":"Not friends","status_code":403}}"#,
        );
        assert_eq!(
            e,
            Error::Api {
                status: 403,
                message: "Not friends".into()
            }
        );
        assert_eq!(e.to_string(), "(403) Not friends");
    }

    #[test]
    fn unauthorized_is_special_cased() {
        let e = Error::from_response(
            401,
            r#"{"error":{"message":"Invalid Username/Email or Password","status_code":401}}"#,
        );
        assert!(e.is_unauthorized());
        assert_eq!(e.kind(), "unauthorized");
        assert_eq!(e.to_string(), "(401) Invalid Username/Email or Password");
    }

    #[test]
    fn non_json_body_falls_back_to_reason() {
        let e = Error::from_response(404, "<html>nope</html>");
        assert_eq!(e.to_string(), "(404) Not Found");
        let e = Error::from_response(418, "");
        assert_eq!(e.to_string(), "(418) Error");
    }
}
