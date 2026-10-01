//! 入力バリデーション (ワールド ID / ユーザー ID / location / 2FA コード)。

use std::sync::LazyLock;

use regex::Regex;

use crate::vrchat::error::{Error, Result};

/// 招待対象の上限人数。
pub const MAX_INVITE_TARGETS: usize = 20;

/// VRChat が使うリージョン。
pub const REGIONS: &[&str] = &["jp", "us", "use", "eu"];

const UUID: &str = "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";

static WORLD_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("^wrld_{UUID}$")).unwrap());
static USER_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("^usr_{UUID}$")).unwrap());
static SIX_DIGITS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{6}$").unwrap());
static RECOVERY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9]{4}-[A-Za-z0-9]{4}$").unwrap());

pub fn is_world_id(s: &str) -> bool {
    WORLD_ID_RE.is_match(s)
}

pub fn is_user_id(s: &str) -> bool {
    USER_ID_RE.is_match(s)
}

pub fn is_six_digit_code(s: &str) -> bool {
    SIX_DIGITS_RE.is_match(s)
}

pub fn is_recovery_code(s: &str) -> bool {
    RECOVERY_RE.is_match(s)
}

/// 2FA コードとして受け付けられる形式か。
pub fn is_two_factor_code(s: &str) -> bool {
    let s = s.trim();
    is_six_digit_code(s) || is_recovery_code(s)
}

/// `wrld_...:instanceId~...` 形式の location を (worldId, instanceId) に分割する。
pub fn split_location(location: &str) -> Result<(&str, &str)> {
    let location = location.trim();
    let (world_id, instance_id) = location
        .split_once(':')
        .ok_or_else(|| Error::Validation(format!("location形式が不正です: {location}")))?;
    if !is_world_id(world_id) {
        return Err(Error::Validation(format!(
            "location形式が不正です (ワールドIDが不正): {location}"
        )));
    }
    if instance_id.is_empty() || instance_id.contains([' ', '/', '?', '#']) {
        return Err(Error::Validation(format!(
            "location形式が不正です (インスタンスIDが不正): {location}"
        )));
    }
    Ok((world_id, instance_id))
}

pub fn validate_world_id(s: &str) -> Result<&str> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::Validation("ワールドIDを入力してください。".into()));
    }
    if !is_world_id(s) {
        return Err(Error::Validation(format!(
            "ワールドIDの形式が正しくありません (wrld_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx): {s}"
        )));
    }
    Ok(s)
}

pub fn validate_user_id(s: &str) -> Result<&str> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::Validation("ユーザーIDを入力してください。".into()));
    }
    if !is_user_id(s) {
        return Err(Error::Validation(format!(
            "ユーザーIDの形式が正しくありません (usr_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx): {s}"
        )));
    }
    Ok(s)
}

pub fn validate_region(s: &str) -> Result<&str> {
    let s = s.trim();
    if REGIONS.contains(&s) {
        Ok(s)
    } else {
        Err(Error::Validation(format!("リージョンが不正です: {s}")))
    }
}

/// 招待対象一覧を検証する (空・上限超過・重複・形式)。
pub fn validate_invite_targets(ids: &[String]) -> Result<Vec<String>> {
    if ids.is_empty() {
        return Err(Error::Validation(
            "ユーザーを選択または入力してください。".into(),
        ));
    }
    if ids.len() > MAX_INVITE_TARGETS {
        return Err(Error::Validation(format!(
            "招待できるのは最大 {MAX_INVITE_TARGETS} 人までです。"
        )));
    }
    let mut out: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        let id = validate_user_id(id)?;
        if !out.iter().any(|x| x == id) {
            out.push(id.to_string());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "wrld_12345678-1234-1234-1234-123456789abc";
    const U: &str = "usr_12345678-1234-1234-1234-123456789abc";

    #[test]
    fn ids() {
        assert!(is_world_id(W));
        assert!(!is_world_id("wrld_short"));
        assert!(!is_world_id(U));
        assert!(is_user_id(U));
        assert!(!is_user_id("usr_"));
        assert!(validate_world_id(&format!("  {W} ")).is_ok());
        assert!(validate_world_id("").is_err());
        assert!(validate_user_id("someone").is_err());
    }

    #[test]
    fn location() {
        let loc = format!("{W}:12345~region(jp)");
        let (w, i) = split_location(&loc).unwrap();
        assert_eq!(w, W);
        assert_eq!(i, "12345~region(jp)");
        assert!(split_location(W).is_err());
        assert!(split_location(&format!("{W}:")).is_err());
        assert!(split_location("wrld_x:1").is_err());
    }

    #[test]
    fn two_factor_codes() {
        assert!(is_two_factor_code("000000"));
        assert!(is_two_factor_code("abcd-1234"));
        assert!(!is_two_factor_code("00000"));
        assert!(!is_two_factor_code("abcd1234"));
    }

    #[test]
    fn invite_targets() {
        assert!(validate_invite_targets(&[]).is_err());
        let many: Vec<String> = (0..21).map(|_| U.to_string()).collect();
        assert!(validate_invite_targets(&many).is_err());
        let dup = vec![U.to_string(), U.to_string()];
        assert_eq!(validate_invite_targets(&dup).unwrap().len(), 1);
        assert!(validate_invite_targets(&["nope".to_string()]).is_err());
        assert!(validate_region("jp").is_ok());
        assert!(validate_region("mars").is_err());
    }
}
