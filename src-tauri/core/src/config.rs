//! `config.json` (ユーザー名のみ。Cookie は別ファイルに暗号化して保存する)。

use serde::{Deserialize, Serialize};

/// 平文で保存してよい設定。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// 最後にログインしたユーザー名 (ログイン画面の初期値)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    // 旧 Python 版の `session_cookies` (平文 Cookie) は読み込まず、保存時に破棄する。
}

impl Config {
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_legacy_plaintext_cookies() {
        let c = Config::parse(r#"{"username":"alice","session_cookies":"[{\"name\":\"auth\"}]"}"#);
        assert_eq!(c.username.as_deref(), Some("alice"));
        assert!(!c.to_json().contains("session_cookies"));
    }

    #[test]
    fn tolerates_garbage() {
        assert_eq!(Config::parse("not json"), Config::default());
        assert_eq!(Config::parse("").username, None);
    }
}
