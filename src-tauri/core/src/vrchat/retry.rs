//! 429 (レート制限) 時のリトライ判断。純粋関数にしてテストしやすくしている。

use std::time::Duration;

/// 最大リトライ回数 (初回リクエストは含まない)。
pub const MAX_RETRIES: u32 = 3;
/// `Retry-After` が無いときの基本待ち時間。
pub const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(5);
/// 待ち時間の上限 (サーバーが異常に大きな値を返しても待ち過ぎない)。
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// `attempt` 回目のリトライ (0 始まり) を行うべきか、行うなら何秒待つかを返す。
///
/// * `retry_after`: レスポンスの `Retry-After` ヘッダ値 (秒数のみ対応。日付形式は無視する)。
/// * 返り値 `None` はリトライ打ち切り。
pub fn retry_delay(attempt: u32, retry_after: Option<&str>) -> Option<Duration> {
    if attempt >= MAX_RETRIES {
        return None;
    }
    let delay = match retry_after.and_then(|s| s.trim().parse::<u64>().ok()) {
        Some(secs) => Duration::from_secs(secs.max(1)),
        // 指数バックオフ: 5s, 10s, 20s
        None => DEFAULT_RETRY_AFTER * 2u32.pow(attempt),
    };
    Some(delay.min(MAX_RETRY_AFTER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honours_retry_after_header() {
        assert_eq!(retry_delay(0, Some("7")), Some(Duration::from_secs(7)));
        assert_eq!(retry_delay(0, Some(" 2 ")), Some(Duration::from_secs(2)));
        // 0 秒は 1 秒に丸める
        assert_eq!(retry_delay(0, Some("0")), Some(Duration::from_secs(1)));
    }

    #[test]
    fn backs_off_exponentially_without_header() {
        assert_eq!(retry_delay(0, None), Some(Duration::from_secs(5)));
        assert_eq!(retry_delay(1, None), Some(Duration::from_secs(10)));
        assert_eq!(retry_delay(2, None), Some(Duration::from_secs(20)));
        assert_eq!(retry_delay(3, None), None);
    }

    #[test]
    fn caps_and_ignores_garbage() {
        assert_eq!(retry_delay(0, Some("99999")), Some(MAX_RETRY_AFTER));
        // 日付形式は解釈せず既定値
        assert_eq!(
            retry_delay(1, Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            Some(Duration::from_secs(10))
        );
    }
}
