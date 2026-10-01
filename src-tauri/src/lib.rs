//! VRCInviteTool (Tauri アプリ本体)。
//!
//! VRChat API との通信とセッション保存は `vrcinvite_core` に置き、ここでは
//! Tauri コマンド (フロントエンドから `invoke` される関数) と共有状態だけを持つ。

mod commands;
mod logging;
mod state;

use state::AppState;

/// アプリのバージョン (src-tauri/Cargo.toml が唯一の情報源。tauri.conf.json は version を持たない)。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new().expect("failed to initialise application state");
    let _log_guard = logging::init(&state.store.log_dir());
    tracing::info!(version = VERSION, data_dir = %state.store.dir().display(), "VRCInviteTool 起動");

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::try_auto_login,
            commands::login,
            commands::submit_two_factor,
            commands::cancel_login,
            commands::logout,
            commands::list_worlds,
            commands::get_world,
            commands::create_instance,
            commands::list_friends,
            commands::get_user,
            commands::invite_users,
            commands::invite_self,
            commands::fetch_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
