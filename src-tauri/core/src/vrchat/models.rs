//! VRChat API のモデル。必要なフィールドだけを取り出し、未知のフィールドは無視する。
//! (API は頻繫にフィールドが増減するため、すべて `Option` + `default` で寛容に受ける)

use serde::{Deserialize, Serialize};

/// `GET /auth/user` のログイン済みユーザー。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUser {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub user_icon: Option<String>,
    #[serde(default)]
    pub current_avatar_thumbnail_image_url: Option<String>,
}

/// `GET /auth/user/friends` の要素 (LimitedUserFriend)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
}

/// `GET /users/{id}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub display_name: String,
    /// vrchatapi 1.21 以降のアイコン URL。
    #[serde(default)]
    pub icon_url: Option<String>,
    // 旧フィールド (サーバーが返す場合のフォールバック)
    #[serde(default)]
    pub profile_pic_override: Option<String>,
    #[serde(default)]
    pub profile_pic_override_thumbnail: Option<String>,
    #[serde(default)]
    pub current_avatar_image_url: Option<String>,
    #[serde(default)]
    pub current_avatar_thumbnail_image_url: Option<String>,
    #[serde(default)]
    pub is_friend: bool,
}

impl User {
    /// 表示に使うアイコン URL。新旧フィールドを優先順に辿る。
    pub fn icon(&self) -> Option<&str> {
        [
            &self.icon_url,
            &self.profile_pic_override,
            &self.profile_pic_override_thumbnail,
            &self.current_avatar_image_url,
            &self.current_avatar_thumbnail_image_url,
        ]
        .into_iter()
        .filter_map(|o| o.as_deref())
        .find(|s| !s.is_empty())
    }
}

/// `GET /worlds/{id}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub thumbnail_image_url: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
}

impl World {
    pub fn thumbnail(&self) -> Option<&str> {
        self.thumbnail_image_url
            .as_deref()
            .or(self.image_url.as_deref())
            .filter(|s| !s.is_empty())
    }
}

/// `GET /worlds/favorites` の要素 (FavoritedWorld)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritedWorld {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub thumbnail_image_url: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub favorite_group: Option<String>,
}

/// `GET /worlds?user=me` の要素 (LimitedWorld)。自作ワールド (非公開含む)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnWorld {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub thumbnail_image_url: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    /// `public` / `private` / `hidden`。
    #[serde(default)]
    pub release_status: Option<String>,
}

/// UI に渡すワールド一覧の要素。お気に入りと自作をマージし、id で重複排除したもの。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub thumbnail_image_url: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    /// お気に入りに含まれる。
    pub favorite: bool,
    /// 自分が作成したワールド。
    pub own: bool,
    #[serde(default)]
    pub release_status: Option<String>,
    #[serde(default)]
    pub favorite_group: Option<String>,
}

/// お気に入り (先) と自作 (後) を結合し、同じ id は 1 件にまとめる (両方に含まれる場合は
/// `favorite` と `own` の両方が true になり、自作側の `releaseStatus` を引き継ぐ)。
pub fn merge_worlds(favorites: Vec<FavoritedWorld>, own: Vec<OwnWorld>) -> Vec<WorldEntry> {
    let mut out: Vec<WorldEntry> = Vec::with_capacity(favorites.len() + own.len());
    for w in favorites {
        if out.iter().any(|e| e.id == w.id) {
            continue;
        }
        out.push(WorldEntry {
            id: w.id,
            name: w.name,
            author_name: w.author_name,
            thumbnail_image_url: w.thumbnail_image_url,
            image_url: w.image_url,
            favorite: true,
            own: false,
            release_status: None,
            favorite_group: w.favorite_group,
        });
    }
    for w in own {
        if let Some(e) = out.iter_mut().find(|e| e.id == w.id) {
            e.own = true;
            e.release_status = w.release_status;
            continue;
        }
        out.push(WorldEntry {
            id: w.id,
            name: w.name,
            author_name: w.author_name,
            thumbnail_image_url: w.thumbnail_image_url,
            image_url: w.image_url,
            favorite: false,
            own: true,
            release_status: w.release_status,
            favorite_group: None,
        });
    }
    out
}

/// API 上のインスタンスタイプ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceType {
    Public,
    Hidden,
    Friends,
    Private,
    Group,
}

/// UI で選ぶインスタンス種別 (Python 版と同じ 5 種)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiInstanceType {
    Public,
    Friends,
    FriendsPlus,
    Invite,
    InvitePlus,
}

impl UiInstanceType {
    /// フロントエンドから渡される値 (`public|friends|hidden|invite|invite_plus`)。
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "public" => Self::Public,
            "friends" => Self::Friends,
            "hidden" | "friends_plus" => Self::FriendsPlus,
            "invite" => Self::Invite,
            "invite_plus" => Self::InvitePlus,
            _ => return None,
        })
    }

    /// 表示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Friends => "Friends",
            Self::FriendsPlus => "Friends+",
            Self::Invite => "Invite",
            Self::InvitePlus => "Invite+",
        }
    }

    /// API のタイプと `canRequestInvite` へのマッピング。
    pub fn to_api(self) -> (InstanceType, bool) {
        match self {
            Self::Public => (InstanceType::Public, false),
            Self::Friends => (InstanceType::Friends, false),
            Self::FriendsPlus => (InstanceType::Hidden, false),
            Self::Invite => (InstanceType::Private, false),
            Self::InvitePlus => (InstanceType::Private, true),
        }
    }
}

/// `POST /instances` のリクエストボディ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceRequest {
    pub world_id: String,
    #[serde(rename = "type")]
    pub instance_type: InstanceType,
    pub region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_request_invite: Option<bool>,
}

impl CreateInstanceRequest {
    /// UI の種別からリクエストを組み立てる。`owner_id` は public 以外で必須 (自分の usr_ ID)。
    pub fn from_ui(world_id: &str, ui_type: UiInstanceType, region: &str, owner_id: &str) -> Self {
        let (instance_type, can_request_invite) = ui_type.to_api();
        Self {
            world_id: world_id.to_string(),
            instance_type,
            region: region.to_string(),
            owner_id: if instance_type == InstanceType::Public {
                None
            } else {
                Some(owner_id.to_string())
            },
            can_request_invite: if can_request_invite { Some(true) } else { None },
        }
    }
}

/// `POST /instances` / `GET /instances/{loc}` のレスポンス。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub world_id: String,
    pub instance_id: String,
    pub location: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default, rename = "type")]
    pub instance_type: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub n_users: Option<u32>,
    #[serde(default)]
    pub capacity: Option<u32>,
}

/// `POST /invite/{userId}` のボディ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRequest {
    pub instance_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_slot: Option<u8>,
}

/// 招待送信の結果 (Notification)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    /// API は他のフィールドと異なり snake_case (`created_at`) で返す。
    #[serde(default, alias = "created_at")]
    pub created_at: Option<String>,
    #[serde(default)]
    pub receiver_user_id: Option<String>,
    #[serde(default)]
    pub sender_user_id: Option<String>,
    #[serde(default, rename = "type")]
    pub notification_type: Option<String>,
}

/// 2FA コードの送信先。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TwoFactorKind {
    /// メールで届く 6 桁コード。
    EmailOtp,
    /// 認証アプリの 6 桁コード。
    Totp,
    /// リカバリーコード (xxxx-xxxx)。
    Otp,
}

impl TwoFactorKind {
    /// `POST /auth/twofactorauth/{segment}/verify` のパスセグメント。
    pub fn path_segment(self) -> &'static str {
        match self {
            Self::EmailOtp => "emailotp",
            Self::Totp => "totp",
            Self::Otp => "otp",
        }
    }

    /// サーバーが要求した方式 (`requiresTwoFactorAuth`) と入力コードの形式から送信先を決める。
    ///
    /// * 6 桁の数字: `emailOtp` が要求されていれば EmailOtp、そうでなければ Totp
    /// * `xxxx-xxxx`: Otp (リカバリーコード)
    /// * それ以外: `None` (形式エラー)
    pub fn infer(code: &str, methods: &[String]) -> Option<Self> {
        let code = code.trim();
        if crate::validate::is_six_digit_code(code) {
            if methods.iter().any(|m| m.eq_ignore_ascii_case("emailOtp")) {
                Some(Self::EmailOtp)
            } else {
                Some(Self::Totp)
            }
        } else if crate::validate::is_recovery_code(code) {
            Some(Self::Otp)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorVerifyRequest {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorVerifyResponse {
    #[serde(default)]
    pub verified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn methods(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn infers_two_factor_kind() {
        assert_eq!(
            TwoFactorKind::infer("123456", &methods(&["emailOtp"])),
            Some(TwoFactorKind::EmailOtp)
        );
        assert_eq!(
            TwoFactorKind::infer(" 123456 ", &methods(&["totp", "otp"])),
            Some(TwoFactorKind::Totp)
        );
        assert_eq!(
            TwoFactorKind::infer("123456", &[]),
            Some(TwoFactorKind::Totp)
        );
        assert_eq!(
            TwoFactorKind::infer("ab12-cd34", &methods(&["totp", "otp"])),
            Some(TwoFactorKind::Otp)
        );
        assert_eq!(
            TwoFactorKind::infer("AB12-CD34", &methods(&["emailOtp"])),
            Some(TwoFactorKind::Otp)
        );
        assert_eq!(TwoFactorKind::infer("12345", &[]), None);
        assert_eq!(TwoFactorKind::infer("1234567", &[]), None);
        assert_eq!(TwoFactorKind::infer("", &[]), None);
        assert_eq!(TwoFactorKind::infer("abcd-efg", &[]), None);
    }

    #[test]
    fn ui_type_maps_to_api() {
        assert_eq!(
            UiInstanceType::parse("public").unwrap().to_api(),
            (InstanceType::Public, false)
        );
        assert_eq!(
            UiInstanceType::parse("friends").unwrap().to_api(),
            (InstanceType::Friends, false)
        );
        assert_eq!(
            UiInstanceType::parse("hidden").unwrap().to_api(),
            (InstanceType::Hidden, false)
        );
        assert_eq!(
            UiInstanceType::parse("invite").unwrap().to_api(),
            (InstanceType::Private, false)
        );
        assert_eq!(
            UiInstanceType::parse("invite_plus").unwrap().to_api(),
            (InstanceType::Private, true)
        );
        assert_eq!(UiInstanceType::parse("group"), None);
    }

    #[test]
    fn create_instance_request_body() {
        let req =
            CreateInstanceRequest::from_ui("wrld_1", UiInstanceType::InvitePlus, "jp", "usr_me");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "worldId": "wrld_1", "type": "private", "region": "jp",
                "ownerId": "usr_me", "canRequestInvite": true
            })
        );

        let req = CreateInstanceRequest::from_ui("wrld_1", UiInstanceType::Public, "us", "usr_me");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "worldId": "wrld_1", "type": "public", "region": "us" })
        );

        let req =
            CreateInstanceRequest::from_ui("wrld_1", UiInstanceType::FriendsPlus, "eu", "usr_me");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["type"], "hidden");
        assert_eq!(json["ownerId"], "usr_me");
        assert!(json.get("canRequestInvite").is_none());
    }

    #[test]
    fn user_icon_prefers_new_field() {
        let u: User = serde_json::from_str(
            r#"{"id":"usr_1","displayName":"A","iconUrl":"","currentAvatarThumbnailImageUrl":"https://x/thumb"}"#,
        )
        .unwrap();
        assert_eq!(u.icon(), Some("https://x/thumb"));
        let u: User =
            serde_json::from_str(r#"{"id":"usr_1","displayName":"A","iconUrl":"https://x/icon"}"#)
                .unwrap();
        assert_eq!(u.icon(), Some("https://x/icon"));
    }

    #[test]
    fn merges_favorites_and_own_worlds_by_id() {
        let fav = |id: &str, name: &str| FavoritedWorld {
            id: id.into(),
            name: name.into(),
            author_name: None,
            thumbnail_image_url: None,
            image_url: None,
            favorite_group: Some("worlds1".into()),
        };
        let own = |id: &str, name: &str, status: &str| OwnWorld {
            id: id.into(),
            name: name.into(),
            author_name: Some("me".into()),
            thumbnail_image_url: None,
            image_url: None,
            release_status: Some(status.into()),
        };
        let merged = merge_worlds(
            vec![
                fav("wrld_a", "A"),
                fav("wrld_b", "B"),
                fav("wrld_a", "A dup"),
            ],
            vec![own("wrld_b", "B", "private"), own("wrld_c", "C", "public")],
        );
        let ids: Vec<&str> = merged.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, vec!["wrld_a", "wrld_b", "wrld_c"]);
        assert!(merged[0].favorite && !merged[0].own);
        assert!(merged[1].favorite && merged[1].own);
        assert_eq!(merged[1].release_status.as_deref(), Some("private"));
        assert_eq!(merged[1].favorite_group.as_deref(), Some("worlds1"));
        assert!(!merged[2].favorite && merged[2].own);
        let json = serde_json::to_value(&merged[2]).unwrap();
        assert_eq!(json["releaseStatus"], "public");
        assert_eq!(json["own"], true);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let f: Friend = serde_json::from_str(
            r#"{"id":"usr_1","displayName":"A","iconUrl":null,"weird":{"x":1},"tags":["a"]}"#,
        )
        .unwrap();
        assert_eq!(f.display_name, "A");
        assert_eq!(f.icon_url, None);
    }
}
