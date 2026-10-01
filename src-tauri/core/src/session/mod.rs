//! セッション (Cookie) とユーザー名の永続化。
//!
//! * 保存先: Windows では `%APPDATA%\VRCInviteTool\`、それ以外では XDG の config dir
//! * `session.bin`: Cookie jar の JSON を DPAPI (CurrentUser) で暗号化したもの
//! * `config.json`: ユーザー名 (平文)
//! * 書き込みは一時ファイル + rename でアトミックに行い、Mutex で直列化する

pub mod dpapi;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::config::Config;
use crate::vrchat::error::{Error, Result};

pub const APP_DIR_NAME: &str = "VRCInviteTool";
pub const SESSION_FILE: &str = "session.bin";
pub const CONFIG_FILE: &str = "config.json";
pub const LOG_DIR: &str = "logs";

/// アプリのデータディレクトリを保持し、読み書きを提供する。
#[derive(Debug)]
pub struct SessionStore {
    dir: PathBuf,
    write_lock: Mutex<()>,
}

impl SessionStore {
    /// 既定のディレクトリ (`%APPDATA%\VRCInviteTool` 等) を使う。
    pub fn open_default() -> Self {
        Self::new(default_data_dir())
    }

    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            write_lock: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn log_dir(&self) -> PathBuf {
        self.dir.join(LOG_DIR)
    }

    fn session_path(&self) -> PathBuf {
        self.dir.join(SESSION_FILE)
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }

    /// ディレクトリを作成する (Unix では 0o700)。
    pub fn ensure_dir(&self) -> Result<()> {
        ensure_private_dir(&self.dir)
    }

    // ---------------- Cookie ----------------

    /// 保存済み Cookie JSON を復号して返す。無ければ `None`。
    /// 復号に失敗した場合 (別ユーザー/別 PC のファイル等) はエラー。
    pub fn load_cookies(&self) -> Result<Option<String>> {
        let path = self.session_path();
        let blob = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Session(format!("{}: {e}", path.display()))),
        };
        if blob.is_empty() {
            return Ok(None);
        }
        let plain = dpapi::unprotect(&blob).map_err(Error::Session)?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|e| Error::Session(format!("セッションファイルが壊れています: {e}")))
    }

    /// Cookie JSON を暗号化して保存する。
    pub fn save_cookies(&self, cookies_json: &str) -> Result<()> {
        let blob = dpapi::protect(cookies_json.as_bytes()).map_err(Error::Session)?;
        self.write_atomic(&self.session_path(), &blob)
    }

    /// 保存済み Cookie を削除する。
    pub fn clear_cookies(&self) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| Error::Session("lock poisoned".into()))?;
        match fs::remove_file(self.session_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Session(format!(
                "セッションファイルを削除できません: {e}"
            ))),
        }
    }

    // ---------------- Config ----------------

    pub fn load_config(&self) -> Config {
        fs::read_to_string(self.config_path())
            .map(|s| Config::parse(&s))
            .unwrap_or_default()
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        self.write_atomic(&self.config_path(), config.to_json().as_bytes())
    }

    pub fn load_username(&self) -> Option<String> {
        self.load_config().username.filter(|s| !s.is_empty())
    }

    pub fn save_username(&self, username: &str) -> Result<()> {
        let mut config = self.load_config();
        config.username = Some(username.to_string());
        self.save_config(&config)
    }

    // ---------------- 低レベル ----------------

    /// 一時ファイルに書いてから rename で置き換える。
    fn write_atomic(&self, path: &Path, data: &[u8]) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| Error::Session("lock poisoned".into()))?;
        self.ensure_dir()?;
        let tmp = unique_tmp_path(path);
        let result = (|| -> std::io::Result<()> {
            let mut file = open_private_file(&tmp)?;
            file.write_all(data)?;
            file.sync_all()?;
            drop(file);
            rename_with_retry(&tmp, path)
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Session(format!(
                "{} の書き込みに失敗しました: {e}",
                path.display()
            )));
        }
        Ok(())
    }
}

/// 既定のデータディレクトリ。Windows: `%APPDATA%\VRCInviteTool`。
pub fn default_data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA").filter(|s| !s.is_empty()) {
        if cfg!(windows) {
            return PathBuf::from(appdata).join(APP_DIR_NAME);
        }
    }
    match directories::BaseDirs::new() {
        Some(base) => base.config_dir().join(APP_DIR_NAME),
        None => PathBuf::from(".").join(format!(".{}", APP_DIR_NAME.to_lowercase())),
    }
}

fn ensure_private_dir(dir: &Path) -> Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| Error::Session(format!("{} を作成できません: {e}", dir.display())))
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
            .map_err(|e| Error::Session(format!("{} を作成できません: {e}", dir.display())))
    }
}

fn open_private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

fn unique_tmp_path(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let name = format!(
        "{}.{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("file"),
        std::process::id(),
        nanos
    );
    path.with_file_name(name)
}

/// Windows では対象ファイルを他プロセスが開いていると rename が失敗するため短時間リトライする。
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last_err = None;
    for i in 0..3 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && i < 2 => {
                std::thread::sleep(Duration::from_millis(50));
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_round_trip_and_clear() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(tmp.path().join("VRCInviteTool"));
        assert_eq!(store.load_cookies().unwrap(), None);
        store.save_cookies(r#"[{"name":"auth"}]"#).unwrap();
        assert_eq!(
            store.load_cookies().unwrap().as_deref(),
            Some(r#"[{"name":"auth"}]"#)
        );
        // 上書き
        store.save_cookies("v2").unwrap();
        assert_eq!(store.load_cookies().unwrap().as_deref(), Some("v2"));
        // 一時ファイルが残っていない
        let leftovers: Vec<_> = fs::read_dir(store.dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        store.clear_cookies().unwrap();
        assert_eq!(store.load_cookies().unwrap(), None);
        store.clear_cookies().unwrap(); // 二重削除も OK
    }

    #[test]
    fn username_round_trip_drops_legacy_cookies() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        fs::write(
            tmp.path().join(CONFIG_FILE),
            r#"{"username":"old","session_cookies":"[{\"name\":\"auth\",\"value\":\"secret\"}]"}"#,
        )
        .unwrap();
        assert_eq!(store.load_username().as_deref(), Some("old"));
        store.save_username("alice").unwrap();
        let raw = fs::read_to_string(tmp.path().join(CONFIG_FILE)).unwrap();
        assert!(raw.contains("alice"));
        assert!(!raw.contains("secret"));
        assert_eq!(store.load_username().as_deref(), Some("alice"));
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("VRCInviteTool");
        let store = SessionStore::new(&dir);
        store.save_cookies("x").unwrap();
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.join(SESSION_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn default_dir_ends_with_app_name() {
        assert!(
            default_data_dir().ends_with(APP_DIR_NAME)
                || default_data_dir().ends_with(".vrcinvitetool")
        );
    }
}
