//! ファイルログ (`%APPDATA%\VRCInviteTool\logs\vrcinvitetool.log.YYYY-MM-DD`)。
//! パスワード・Cookie 値は決してログに出さない。

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// 保持する日次ログファイルの上限 (古いものから削除される)。
const MAX_LOG_FILES: usize = 14;

/// ログを初期化する。返り値の guard はプロセス終了までドロップしないこと。
pub fn init(log_dir: &Path) -> Option<WorkerGuard> {
    let filter =
        EnvFilter::try_from_env("VRCINVITETOOL_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    let appender = std::fs::create_dir_all(log_dir)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            tracing_appender::rolling::Builder::new()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("vrcinvitetool.log")
                .max_log_files(MAX_LOG_FILES)
                .build(log_dir)
                .map_err(|e| e.to_string())
        });
    let file_layer = match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_timer(UtcTime::rfc_3339())
                .with_writer(writer);
            Some((layer, guard))
        }
        Err(e) => {
            eprintln!("ファイルログを初期化できません: {}: {e}", log_dir.display());
            None
        }
    };

    let stderr_layer = if cfg!(debug_assertions) {
        Some(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_writer(std::io::stderr),
        )
    } else {
        None
    };

    let (file_layer, guard) = match file_layer {
        Some((l, g)) => (Some(l), Some(g)),
        None => (None, None),
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init();
    guard
}
