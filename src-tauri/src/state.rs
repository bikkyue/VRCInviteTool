//! アプリ全体の共有状態。

use serde::Serialize;
use tokio::sync::Mutex;

use vrcinvite_core::session::SessionStore;
use vrcinvite_core::vrchat::models::CurrentUser;
use vrcinvite_core::{Error, VrcClient};

/// 2FA 入力の最大試行回数 (API に拒否された回数のみカウント)。
pub const MAX_TWO_FACTOR_ATTEMPTS: u32 = 3;

/// ログイン途中 (2FA 待ち) の情報。
#[derive(Debug, Clone)]
pub struct PendingLogin {
    /// サーバーが要求した方式 (`requiresTwoFactorAuth`)。
    pub methods: Vec<String>,
    /// API に拒否された回数。
    pub attempts: u32,
    /// ログイン成功時に保存するユーザー名 (自動ログイン経由の場合は None)。
    pub username: Option<String>,
}

#[derive(Debug, Default)]
pub struct Inner {
    pub user: Option<CurrentUser>,
    pub pending: Option<PendingLogin>,
}

pub struct AppState {
    pub client: VrcClient,
    pub store: SessionStore,
    pub inner: Mutex<Inner>,
}

impl AppState {
    pub fn new() -> Result<Self, Error> {
        let client = VrcClient::new(&vrcinvite_core::user_agent(crate::VERSION))?;
        let store = SessionStore::open_default();
        Ok(Self {
            client,
            store,
            inner: Mutex::new(Inner::default()),
        })
    }

    /// ログイン状態をローカルで破棄する (サーバー側 logout は呼ばない)。
    pub async fn reset_local_session(&self) {
        self.client.clear_cookies();
        if let Err(e) = self.store.clear_cookies() {
            tracing::warn!(error = %e, "保存済みセッションの削除に失敗");
        }
        let mut inner = self.inner.lock().await;
        inner.user = None;
        inner.pending = None;
    }

    /// ログイン済みユーザーを返す。未ログインなら Unauthorized。
    pub async fn current_user(&self) -> Result<CurrentUser, Error> {
        self.inner
            .lock()
            .await
            .user
            .clone()
            .ok_or_else(|| Error::Unauthorized {
                message: "ログインしていません".into(),
            })
    }
}

/// フロントエンドへ返すエラー。`kind` で分岐 (unauthorized → ログイン画面へ)。
#[derive(Debug, Clone, Serialize)]
pub struct CmdError {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl CmdError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.into(),
            status: None,
        }
    }
}

impl From<Error> for CmdError {
    fn from(e: Error) -> Self {
        let message = match &e {
            Error::Unauthorized { .. } => {
                "セッションが切れました。再度ログインしてください。".to_string()
            }
            other => other.to_string(),
        };
        Self {
            kind: e.kind().to_string(),
            message,
            status: e.status(),
        }
    }
}
