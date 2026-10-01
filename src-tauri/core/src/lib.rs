//! VRCInviteTool のコア: VRChat API クライアントとセッション (Cookie) の永続化。
//!
//! このクレートは Tauri に依存しないため、Windows 以外でも `cargo test` できる。
//! DPAPI 暗号化だけは Windows 専用で、それ以外の OS ではパーミッション 0o600 の
//! 平文ファイルにフォールバックする (開発用)。

pub mod config;
pub mod session;
pub mod validate;
pub mod vrchat;

pub use vrchat::client::VrcClient;
pub use vrchat::error::{Error, Result};

/// VRChat API に送る User-Agent。`version` はアプリ側の `CARGO_PKG_VERSION` を渡す
/// (バージョンの情報源は src-tauri/Cargo.toml のみ)。
pub fn user_agent(version: &str) -> String {
    format!("VRCInviteTool/{version} (github.com/Droplet-Collective/VRCInviteTool)")
}

#[cfg(test)]
mod tests {
    #[test]
    fn user_agent_contains_version_and_repo() {
        let ua = super::user_agent("1.2.3");
        assert_eq!(
            ua,
            "VRCInviteTool/1.2.3 (github.com/Droplet-Collective/VRCInviteTool)"
        );
    }
}
