//! フロントエンドから `invoke` される Tauri コマンド。
//!
//! すべて async で、共有状態は [`AppState`]。エラーは [`CmdError`] (kind + message) で返し、
//! `kind == "unauthorized"` のときはローカルのセッションを破棄済みなので UI はログイン画面へ戻る。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use vrcinvite_core::validate;
use vrcinvite_core::vrchat::models::*;
use vrcinvite_core::vrchat::AuthResponse;
use vrcinvite_core::Error;

use crate::state::{AppState, CmdError, PendingLogin, MAX_TWO_FACTOR_ATTEMPTS};

type CmdResult<T> = Result<T, CmdError>;

/// 複数人招待時の各送信間の待ち時間 (Python 版と同じ 1 秒)。
const INVITE_INTERVAL: Duration = Duration::from_secs(1);

// ----------------------------------------------------------------------
// ペイロード
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    /// 前回ログインしたユーザー名 (ログイン画面の初期値)。
    pub saved_username: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: String,
    pub display_name: String,
}

impl From<&CurrentUser> for UserSummary {
    fn from(u: &CurrentUser) -> Self {
        Self {
            id: u.id.clone(),
            display_name: u.display_name.clone(),
        }
    }
}

/// ログイン系コマンドの結果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginOutcome {
    LoggedIn {
        user: UserSummary,
    },
    /// 2FA コードの入力が必要。`email` はメール OTP かどうか (文言の出し分け用)。
    TwoFactorRequired {
        methods: Vec<String>,
        email: bool,
    },
    /// 保存済みセッションが無い/無効 (自動ログインのみ)。
    NotLoggedIn {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct InviteTarget {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteResult {
    pub user_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteReport {
    pub results: Vec<InviteResult>,
    pub succeeded: usize,
    pub failed: usize,
    /// 致命的エラーで途中中断した場合 true (未送信の相手が残っている)。
    pub aborted: bool,
}

// ----------------------------------------------------------------------
// 共通ヘルパー
// ----------------------------------------------------------------------

/// ログパネル (フロントエンド) とファイルログの両方に出す。
fn ui_log(app: &AppHandle, message: impl Into<String>) {
    let message = message.into();
    tracing::info!("{message}");
    if let Err(e) = app.emit("log", LogEvent { message }) {
        tracing::warn!(error = %e, "log イベントの送信に失敗");
    }
}

/// エラーを UI 向けに変換する。401 ならローカルのセッションを破棄する。
async fn fail(state: &AppState, e: Error) -> CmdError {
    if e.is_unauthorized() {
        tracing::warn!("401 を受信: セッションを破棄してログイン画面に戻します");
        state.reset_local_session().await;
    } else {
        tracing::warn!(error = %e, kind = e.kind(), "コマンド失敗");
    }
    CmdError::from(e)
}

fn is_email_method(methods: &[String]) -> bool {
    methods.iter().any(|m| m.eq_ignore_ascii_case("emailOtp"))
}

/// ログイン完了時の共通処理: 状態更新・セッション保存・ユーザー名保存。
async fn finish_login(
    app: &AppHandle,
    state: &AppState,
    user: CurrentUser,
    username: Option<&str>,
    reused: bool,
) -> CmdResult<LoginOutcome> {
    match state.client.export_cookies() {
        Ok(json) => {
            if let Err(e) = state.store.save_cookies(&json) {
                // 保存できなくてもログイン自体は成立しているので警告のみ
                ui_log(app, format!("警告: セッションを保存できませんでした {e}"));
            }
        }
        Err(e) => ui_log(app, format!("警告: セッションを保存できませんでした {e}")),
    }
    if let Some(name) = username {
        if let Err(e) = state.store.save_username(name) {
            tracing::warn!(error = %e, "ユーザー名の保存に失敗");
        }
    }
    let summary = UserSummary::from(&user);
    {
        let mut inner = state.inner.lock().await;
        inner.user = Some(user);
        inner.pending = None;
    }
    if reused {
        ui_log(app, format!("セッション再利用: {}", summary.display_name));
    } else {
        ui_log(app, format!("ログイン成功: {}", summary.display_name));
    }
    Ok(LoginOutcome::LoggedIn { user: summary })
}

// ----------------------------------------------------------------------
// アプリ情報
// ----------------------------------------------------------------------

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        version: crate::VERSION.to_string(),
        data_dir: state.store.dir().display().to_string(),
        saved_username: state.store.load_username(),
    })
}

// ----------------------------------------------------------------------
// 認証
// ----------------------------------------------------------------------

/// 起動時の自動ログイン。保存済み Cookie を復元して `GET /auth/user` を試す。
/// `twoFactorAuth` Cookie だけが失効している場合は auth Cookie を残したまま 2FA 入力を求める。
#[tauri::command]
pub async fn try_auto_login(app: AppHandle, state: State<'_, AppState>) -> CmdResult<LoginOutcome> {
    let json = match state.store.load_cookies() {
        Ok(Some(json)) => json,
        Ok(None) => return Ok(LoginOutcome::NotLoggedIn { message: None }),
        Err(e) => {
            tracing::warn!(error = %e, "保存済みセッションを読めません。削除します");
            let _ = state.store.clear_cookies();
            return Ok(LoginOutcome::NotLoggedIn {
                message: Some(e.to_string()),
            });
        }
    };
    if let Err(e) = state.client.import_cookies(&json) {
        tracing::warn!(error = %e, "保存済みセッションを復元できません。削除します");
        let _ = state.store.clear_cookies();
        return Ok(LoginOutcome::NotLoggedIn { message: None });
    }
    if !state.client.has_cookie("auth") {
        tracing::info!("auth Cookie が期限切れのため再ログインが必要です");
        state.reset_local_session().await;
        return Ok(LoginOutcome::NotLoggedIn { message: None });
    }
    tracing::info!(cookies = ?state.client.cookie_names(), "保存済みセッションで認証を試行");

    match state.client.get_current_user(None).await {
        Ok(AuthResponse::LoggedIn(user)) => finish_login(&app, &state, user, None, true).await,
        Ok(AuthResponse::TwoFactorRequired { methods }) => {
            tracing::info!(
                ?methods,
                "twoFactorAuth Cookie が無効のため 2FA のみ再入力を要求"
            );
            let email = is_email_method(&methods);
            let mut inner = state.inner.lock().await;
            inner.user = None;
            inner.pending = Some(PendingLogin {
                methods: methods.clone(),
                attempts: 0,
                username: None,
            });
            Ok(LoginOutcome::TwoFactorRequired { methods, email })
        }
        Err(Error::Unauthorized { message }) => {
            tracing::info!(%message, "保存済みセッションが無効。再ログインします");
            state.reset_local_session().await;
            Ok(LoginOutcome::NotLoggedIn {
                message: Some("保存済みセッションが無効です。再ログインしてください。".into()),
            })
        }
        Err(e) => Err(fail(&state, e).await),
    }
}

/// ユーザー名・パスワードでログインする。保存済みセッションは先に破棄する
/// (入力した認証情報と別アカウントの Cookie でログインしないため)。
#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> CmdResult<LoginOutcome> {
    let username = username.trim().to_string();
    if username.is_empty() || password.is_empty() {
        return Err(CmdError::new(
            "validation",
            "ユーザー名とパスワードを入力してください。",
        ));
    }
    state.reset_local_session().await;
    tracing::info!("ログイン開始");

    let result = state
        .client
        .get_current_user(Some((&username, &password)))
        .await;
    // パスワードはここで用済み。以降は Cookie で認証する (Basic 認証ヘッダは再送しない)。
    drop(password);

    match result {
        Ok(AuthResponse::LoggedIn(user)) => {
            finish_login(&app, &state, user, Some(&username), false).await
        }
        Ok(AuthResponse::TwoFactorRequired { methods }) => {
            tracing::info!(?methods, "2FA が必要");
            let email = is_email_method(&methods);
            let mut inner = state.inner.lock().await;
            inner.pending = Some(PendingLogin {
                methods: methods.clone(),
                attempts: 0,
                username: Some(username),
            });
            Ok(LoginOutcome::TwoFactorRequired { methods, email })
        }
        Err(e @ Error::Unauthorized { .. }) => {
            tracing::info!(error = %e, "認証失敗");
            state.client.clear_cookies();
            Err(CmdError::new(
                "login_failed",
                format!("ログインに失敗しました {e}"),
            ))
        }
        Err(e) => {
            state.client.clear_cookies();
            Err(fail(&state, e).await)
        }
    }
}

/// 2FA コードを送信する。6 桁は totp/emailOtp (サーバーの要求に従う)、`xxxx-xxxx` はリカバリーコード。
/// 形式エラーは試行回数を消費しない。API に 3 回拒否されたらログインをやり直させる。
#[tauri::command]
pub async fn submit_two_factor(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
) -> CmdResult<LoginOutcome> {
    let pending = state.inner.lock().await.pending.clone().ok_or_else(|| {
        CmdError::new(
            "login",
            "ログイン処理が開始されていません。ログインをやり直してください。",
        )
    })?;

    let kind = TwoFactorKind::infer(&code, &pending.methods).ok_or_else(|| {
        CmdError::new(
            "two_factor_format",
            "2FAコードの形式が正しくありません (6桁の数字、またはリカバリーコード xxxx-xxxx)。",
        )
    })?;
    tracing::info!(?kind, "2FA コードを送信");

    let verified = match state.client.verify_two_factor(kind, &code).await {
        Ok(v) => v,
        Err(e) => return Err(fail(&state, e).await),
    };

    if !verified {
        let attempts = pending.attempts + 1;
        if attempts >= MAX_TWO_FACTOR_ATTEMPTS {
            tracing::warn!(attempts, "2FA の試行回数上限");
            state.reset_local_session().await;
            return Err(CmdError::new(
                "two_factor_failed",
                "2FAコードの確認に失敗しました。ログインをやり直してください。",
            ));
        }
        if let Some(p) = state.inner.lock().await.pending.as_mut() {
            p.attempts = attempts;
        }
        let remaining = MAX_TWO_FACTOR_ATTEMPTS - attempts;
        return Err(CmdError::new(
            "two_factor_invalid",
            format!("2FAコードが正しくありません。(残り {remaining} 回)"),
        ));
    }

    match state.client.get_current_user(None).await {
        Ok(AuthResponse::LoggedIn(user)) => {
            finish_login(&app, &state, user, pending.username.as_deref(), false).await
        }
        Ok(AuthResponse::TwoFactorRequired { .. }) => {
            state.reset_local_session().await;
            Err(CmdError::new(
                "login",
                "2FA 確認後もログインできませんでした。ログインをやり直してください。",
            ))
        }
        Err(e @ Error::Unauthorized { .. }) => {
            state.reset_local_session().await;
            Err(CmdError::new(
                "login_failed",
                format!("ログインに失敗しました {e}"),
            ))
        }
        Err(e) => Err(fail(&state, e).await),
    }
}

/// 2FA ダイアログのキャンセル。進行中のログインを破棄する。
#[tauri::command]
pub async fn cancel_login(state: State<'_, AppState>) -> CmdResult<()> {
    tracing::info!("ログインをキャンセル");
    state.client.clear_cookies();
    let mut inner = state.inner.lock().await;
    inner.pending = None;
    inner.user = None;
    Ok(())
}

/// サーバー側セッションを無効化し、ローカルの保存分も破棄する。
#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if state.client.has_cookie("auth") {
        if let Err(e) = state.client.logout().await {
            ui_log(&app, format!("サーバー側ログアウトに失敗: {e}"));
        }
    }
    state.reset_local_session().await;
    ui_log(&app, "ログアウトしました。");
    Ok(())
}

// ----------------------------------------------------------------------
// データ取得
// ----------------------------------------------------------------------

/// お気に入りワールドと自作ワールドを取得して結合する (id で重複排除)。
/// 自作ワールドの取得だけが失敗した場合 (401 以外) は警告を出してお気に入りのみ返す。
#[tauri::command]
pub async fn list_worlds(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<WorldEntry>> {
    state.current_user().await.map_err(CmdError::from)?;
    let favorites = match state.client.list_favorite_worlds().await {
        Ok(v) => v,
        Err(e) => return Err(fail(&state, e).await),
    };
    let own = match state.client.list_own_worlds().await {
        Ok(v) => v,
        Err(e @ Error::Unauthorized { .. }) => return Err(fail(&state, e).await),
        Err(e) => {
            ui_log(&app, format!("警告: 自作ワールドの取得に失敗しました: {e}"));
            Vec::new()
        }
    };
    Ok(merge_worlds(favorites, own))
}

#[tauri::command]
pub async fn list_friends(state: State<'_, AppState>) -> CmdResult<Vec<Friend>> {
    state.current_user().await.map_err(CmdError::from)?;
    match state.client.list_friends().await {
        Ok(v) => Ok(v),
        Err(e) => Err(fail(&state, e).await),
    }
}

#[tauri::command]
pub async fn get_world(state: State<'_, AppState>, world_id: String) -> CmdResult<World> {
    state.current_user().await.map_err(CmdError::from)?;
    match state.client.get_world(&world_id).await {
        Ok(v) => Ok(v),
        Err(e) => Err(fail(&state, e).await),
    }
}

#[tauri::command]
pub async fn get_user(state: State<'_, AppState>, user_id: String) -> CmdResult<User> {
    state.current_user().await.map_err(CmdError::from)?;
    match state.client.get_user(&user_id).await {
        Ok(v) => Ok(v),
        Err(e) => Err(fail(&state, e).await),
    }
}

/// サムネイル画像を `data:` URL で返す (`*.vrchat.cloud` のみ)。
#[tauri::command]
pub async fn fetch_image(state: State<'_, AppState>, url: String) -> CmdResult<String> {
    match state.client.fetch_image_data_url(&url).await {
        Ok(v) => Ok(v),
        Err(e) => Err(fail(&state, e).await),
    }
}

// ----------------------------------------------------------------------
// インスタンス作成
// ----------------------------------------------------------------------

#[tauri::command]
pub async fn create_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    world_id: String,
    instance_type: String,
    region: String,
) -> CmdResult<Instance> {
    let user = state.current_user().await.map_err(CmdError::from)?;
    let world_id = validate::validate_world_id(&world_id).map_err(CmdError::from)?;
    let region = validate::validate_region(&region).map_err(CmdError::from)?;
    let ui_type = UiInstanceType::parse(&instance_type).ok_or_else(|| {
        CmdError::new(
            "validation",
            format!("インスタンスタイプが不正です: {instance_type}"),
        )
    })?;
    let request = CreateInstanceRequest::from_ui(world_id, ui_type, region, &user.id);

    ui_log(&app, "インスタンスを作成中...");
    ui_log(&app, format!("  ワールドID : {world_id}"));
    ui_log(&app, format!("  タイプ     : {}", ui_type.label()));
    ui_log(&app, format!("  リージョン : {region}"));

    match state.client.create_instance(&request).await {
        Ok(instance) => {
            ui_log(&app, "--- 作成されたインスタンス ---");
            ui_log(&app, format!("  インスタンスID : {}", instance.instance_id));
            ui_log(&app, format!("  ワールドID     : {}", instance.world_id));
            ui_log(&app, format!("  タイプ         : {}", ui_type.label()));
            ui_log(
                &app,
                format!(
                    "  リージョン     : {}",
                    instance.region.clone().unwrap_or_default()
                ),
            );
            ui_log(&app, format!("  場所           : {}", instance.location));
            Ok(instance)
        }
        Err(e) => Err(fail(&state, e).await),
    }
}

// ----------------------------------------------------------------------
// 招待
// ----------------------------------------------------------------------

/// 複数ユーザーを招待する。各送信の間に 1 秒待つ。
/// API エラー (403 等) はその相手だけ失敗として続行し、通信エラーは中断する。
/// 401 はそれまでの集計をログに出してから `unauthorized` を返す。
#[tauri::command]
pub async fn invite_users(
    app: AppHandle,
    state: State<'_, AppState>,
    location: String,
    targets: Vec<InviteTarget>,
) -> CmdResult<InviteReport> {
    state.current_user().await.map_err(CmdError::from)?;
    let location = location.trim().to_string();
    if location.is_empty() {
        return Err(CmdError::new(
            "validation",
            "インスタンスIDを入力してください。",
        ));
    }
    validate::split_location(&location).map_err(CmdError::from)?;
    let ids: Vec<String> = targets.iter().map(|t| t.id.trim().to_string()).collect();
    let ids = validate::validate_invite_targets(&ids).map_err(CmdError::from)?;
    let name_of = |id: &str| -> String {
        targets
            .iter()
            .find(|t| t.id.trim() == id)
            .and_then(|t| t.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.to_string())
    };

    let total = ids.len();
    ui_log(&app, format!("{total}人に招待を送信します..."));
    let mut results: Vec<InviteResult> = Vec::with_capacity(total);
    let mut aborted = false;

    for (i, uid) in ids.iter().enumerate() {
        let name = name_of(uid);
        ui_log(&app, format!("[{}/{total}] {name} に招待中...", i + 1));
        match state.client.invite_user(uid, &location).await {
            Ok(n) => {
                ui_log(&app, format!("  招待を送信しました (通知ID: {})", n.id));
                results.push(InviteResult {
                    user_id: uid.clone(),
                    ok: true,
                    message: None,
                });
            }
            Err(e @ Error::Unauthorized { .. }) => {
                let sent = results.iter().filter(|r| r.ok).count();
                let failed = results.len() - sent;
                ui_log(
                    &app,
                    format!(
                        "セッションが切れました (送信済み: 成功 {sent} 件 / 失敗 {failed} 件、未送信 {} 件)。再ログインしてください。",
                        total - i
                    ),
                );
                return Err(fail(&state, e).await);
            }
            Err(Error::Api {
                status: 403,
                message,
            }) => {
                ui_log(
                    &app,
                    format!(
                        "  エラー: {name} はフレンドではないため招待できません。 (403) {message}"
                    ),
                );
                results.push(InviteResult {
                    user_id: uid.clone(),
                    ok: false,
                    message: Some(format!("(403) {message}")),
                });
            }
            Err(e @ Error::Api { .. }) | Err(e @ Error::Validation(_)) => {
                ui_log(&app, format!("  APIエラー: {e}"));
                results.push(InviteResult {
                    user_id: uid.clone(),
                    ok: false,
                    message: Some(e.to_string()),
                });
            }
            Err(e) => {
                // 通信エラー等の致命的なエラーはループ全体を中断する
                ui_log(&app, format!("  エラー: {e} — 招待を中断します。"));
                results.push(InviteResult {
                    user_id: uid.clone(),
                    ok: false,
                    message: Some(e.to_string()),
                });
                aborted = true;
                break;
            }
        }
        if i + 1 < total {
            tokio::time::sleep(INVITE_INTERVAL).await;
        }
    }

    let succeeded = results.iter().filter(|r| r.ok).count();
    let failed = results.iter().filter(|r| !r.ok).count();
    if aborted {
        ui_log(
            &app,
            format!(
                "招待を中断しました: 成功 {succeeded} 件 / 失敗 {failed} 件 / 未送信 {} 件",
                total - results.len()
            ),
        );
    } else if failed > 0 {
        let names: Vec<String> = results
            .iter()
            .filter(|r| !r.ok)
            .map(|r| name_of(&r.user_id))
            .collect();
        ui_log(
            &app,
            format!(
                "招待完了: 成功 {succeeded} 件 / 失敗 {failed} 件 ({})",
                names.join(", ")
            ),
        );
        ui_log(&app, "失敗した相手は選択状態に残しています。");
    } else {
        ui_log(
            &app,
            format!("全ての招待を送信しました (成功 {succeeded} 件)。"),
        );
    }
    Ok(InviteReport {
        results,
        succeeded,
        failed,
        aborted,
    })
}

/// 自分自身をインスタンスに招待する (フレンド関係不要)。
#[tauri::command]
pub async fn invite_self(
    app: AppHandle,
    state: State<'_, AppState>,
    location: String,
) -> CmdResult<Notification> {
    state.current_user().await.map_err(CmdError::from)?;
    let location = location.trim().to_string();
    if location.is_empty() {
        return Err(CmdError::new(
            "validation",
            "インスタンスIDを入力してください。",
        ));
    }
    let (world_id, instance_id) = validate::split_location(&location).map_err(CmdError::from)?;
    ui_log(&app, "自分への招待を送信中...");
    ui_log(&app, format!("  ワールドID    : {world_id}"));
    ui_log(&app, format!("  インスタンスID: {instance_id}"));
    match state.client.invite_myself(&location).await {
        Ok(n) => {
            ui_log(&app, "自分への招待を送信しました。");
            ui_log(&app, format!("  通知ID   : {}", n.id));
            if let Some(t) = &n.created_at {
                ui_log(&app, format!("  送信日時 : {t}"));
            }
            Ok(n)
        }
        Err(e) => Err(fail(&state, e).await),
    }
}
