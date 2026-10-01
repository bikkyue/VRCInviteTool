//! wiremock を使った HTTP レベルのテスト (ログイン + 2FA、429 リトライ、401)。

use vrcinvite_core::vrchat::models::*;
use vrcinvite_core::vrchat::{AuthResponse, Error, VrcClient};
use wiremock::matchers::{body_json, header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn client(server: &MockServer) -> VrcClient {
    VrcClient::with_base_url(
        &format!("{}/api/1/", server.uri()),
        "VRCInviteTool/test (test)",
    )
    .unwrap()
}

fn user_json(id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "displayName": name, "username": name.to_lowercase(), "extraField": 1 })
}

#[tokio::test]
async fn login_with_email_2fa_flow() {
    let server = MockServer::start().await;
    let c = client(&server).await;

    // 1. Basic 認証 → 2FA 要求 + auth Cookie
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user"))
        .and(header_exists("authorization"))
        .and(header("user-agent", "VRCInviteTool/test (test)"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "set-cookie",
                    "auth=authcookie; Path=/; HttpOnly; Max-Age=31536000",
                )
                .set_body_json(serde_json::json!({ "requiresTwoFactorAuth": ["emailOtp"] })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let r = c
        .get_current_user(Some(("alice@example.com", "p@ss word")))
        .await
        .unwrap();
    assert_eq!(
        r,
        AuthResponse::TwoFactorRequired {
            methods: vec!["emailOtp".into()]
        }
    );
    assert!(c.has_cookie("auth"));

    // Basic 認証ヘッダは URL エンコード後に Base64 化されている
    let reqs = server.received_requests().await.unwrap();
    let auth = reqs[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    use base64::Engine as _;
    let expected =
        base64::engine::general_purpose::STANDARD.encode("alice%40example.com:p%40ss%20word");
    assert_eq!(auth, format!("Basic {expected}"));

    // 2. 誤ったコード → 400 → Ok(false)
    Mock::given(method("POST"))
        .and(path("/api/1/auth/twofactorauth/emailotp/verify"))
        .and(body_json(serde_json::json!({ "code": "000000" })))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": { "message": "Invalid code", "status_code": 400 }
        })))
        .mount(&server)
        .await;
    let kind = TwoFactorKind::infer("000000", &["emailOtp".to_string()]).unwrap();
    assert_eq!(kind, TwoFactorKind::EmailOtp);
    assert!(!c.verify_two_factor(kind, "000000").await.unwrap());

    // 3. 正しいコード → verified + twoFactorAuth Cookie
    Mock::given(method("POST"))
        .and(path("/api/1/auth/twofactorauth/emailotp/verify"))
        .and(body_json(serde_json::json!({ "code": "123456" })))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "set-cookie",
                    "twoFactorAuth=tfa; Path=/; HttpOnly; Max-Age=2592000",
                )
                .set_body_json(serde_json::json!({ "verified": true })),
        )
        .mount(&server)
        .await;
    assert!(c.verify_two_factor(kind, " 123456 ").await.unwrap());
    assert!(c.has_cookie("twoFactorAuth"));

    // 4. Cookie のみで GET /auth/user → ログイン完了 (Basic ヘッダは送らない)
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user"))
        .and(header_exists("cookie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user_json("usr_1", "Alice")))
        .mount(&server)
        .await;
    match c.get_current_user(None).await.unwrap() {
        AuthResponse::LoggedIn(u) => assert_eq!(u.display_name, "Alice"),
        other => panic!("unexpected {other:?}"),
    }
    let last = server.received_requests().await.unwrap().pop().unwrap();
    assert!(last.headers.get("authorization").is_none());
    let cookie = last.headers.get("cookie").unwrap().to_str().unwrap();
    assert!(
        cookie.contains("auth=authcookie"),
        "cookie header: {cookie}"
    );
    assert!(
        cookie.contains("twoFactorAuth=tfa"),
        "cookie header: {cookie}"
    );

    // 5. エクスポート → 新しいクライアントに復元してもログイン状態を引き継げる
    let json = c.export_cookies().unwrap();
    let c2 = client(&server).await;
    c2.import_cookies(&json).unwrap();
    assert!(c2.has_cookie("auth") && c2.has_cookie("twoFactorAuth"));
}

#[tokio::test]
async fn recovery_code_goes_to_otp_endpoint_and_totp_default() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/1/auth/twofactorauth/otp/verify"))
        .and(body_json(serde_json::json!({ "code": "abcd-1234" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "verified": true })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/1/auth/twofactorauth/totp/verify"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "verified": true })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let methods = vec!["totp".to_string(), "otp".to_string()];
    let kind = TwoFactorKind::infer("abcd-1234", &methods).unwrap();
    assert!(c.verify_two_factor(kind, "abcd-1234").await.unwrap());
    let kind = TwoFactorKind::infer("654321", &methods).unwrap();
    assert!(c.verify_two_factor(kind, "654321").await.unwrap());
}

#[tokio::test]
async fn wrong_password_is_unauthorized_with_body_message() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "message": "Invalid Username/Email or Password", "status_code": 401 }
        })))
        .mount(&server)
        .await;
    let err = c.get_current_user(Some(("a", "b"))).await.unwrap_err();
    assert_eq!(
        err,
        Error::Unauthorized {
            message: "Invalid Username/Email or Password".into()
        }
    );
    assert_eq!(err.to_string(), "(401) Invalid Username/Email or Password");
}

#[tokio::test]
async fn expired_session_returns_unauthorized_on_any_endpoint() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user/friends"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "message": "Missing Credentials", "status_code": 401 }
        })))
        .mount(&server)
        .await;
    let err = c.list_friends().await.unwrap_err();
    assert!(err.is_unauthorized());
    assert_eq!(err.kind(), "unauthorized");
}

#[tokio::test]
async fn retries_on_429_with_retry_after() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    // 1 回目 429 (Retry-After: 0 → 最小 1 秒待ち)、2 回目 200
    Mock::given(method("GET"))
        .and(path("/api/1/worlds/wrld_12345678-1234-1234-1234-123456789abc"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0").set_body_json(
            serde_json::json!({ "error": { "message": "Too Many Requests", "status_code": 429 } }),
        ))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/1/worlds/wrld_12345678-1234-1234-1234-123456789abc",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "wrld_12345678-1234-1234-1234-123456789abc", "name": "Test World",
            "thumbnailImageUrl": "https://api.vrchat.cloud/api/1/image/file_x/1/256"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let w = c
        .get_world("wrld_12345678-1234-1234-1234-123456789abc")
        .await
        .unwrap();
    assert_eq!(w.name, "Test World");
    assert!(started.elapsed() >= std::time::Duration::from_millis(900));
}

#[tokio::test]
async fn gives_up_after_max_retries_on_429() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/1/users/usr_12345678-1234-1234-1234-123456789abc"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0").set_body_json(
            serde_json::json!({ "error": { "message": "Too Many Requests", "status_code": 429 } }),
        ))
        .expect(4) // 初回 + 3 リトライ
        .mount(&server)
        .await;
    let err = c
        .get_user("usr_12345678-1234-1234-1234-123456789abc")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        Error::Api {
            status: 429,
            message: "Too Many Requests".into()
        }
    );
}

#[tokio::test]
async fn friends_pagination_and_favorites() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    let page: Vec<serde_json::Value> = (0..100)
        .map(|i| serde_json::json!({ "id": format!("usr_{i}"), "displayName": format!("F{i}"), "iconUrl": null }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user/friends"))
        .and(query_param("offline", "false"))
        .and(query_param("n", "100"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user/friends"))
        .and(query_param("offline", "false"))
        .and(query_param("offset", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": "usr_x", "displayName": "Last" }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/1/auth/user/friends"))
        .and(query_param("offline", "true"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": "usr_off", "displayName": "Offline" }
        ])))
        .mount(&server)
        .await;
    let friends = c.list_friends().await.unwrap();
    assert_eq!(friends.len(), 102);
    assert_eq!(friends.last().unwrap().display_name, "Offline");

    Mock::given(method("GET"))
        .and(path("/api/1/worlds/favorites"))
        .and(query_param("n", "100"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": "wrld_1", "name": "W1", "favoriteGroup": "worlds1" }
        ])))
        .mount(&server)
        .await;
    let worlds = c.list_favorite_worlds().await.unwrap();
    assert_eq!(worlds.len(), 1);
    assert_eq!(worlds[0].favorite_group.as_deref(), Some("worlds1"));

    // 自作ワールド: user=me & releaseStatus=all (非公開も含む)
    Mock::given(method("GET"))
        .and(path("/api/1/worlds"))
        .and(query_param("user", "me"))
        .and(query_param("releaseStatus", "all"))
        .and(query_param("n", "100"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": "wrld_1", "name": "W1", "authorName": "me", "releaseStatus": "public" },
            { "id": "wrld_2", "name": "Secret", "authorName": "me", "releaseStatus": "private",
              "thumbnailImageUrl": "https://api.vrchat.cloud/api/1/image/file_y/1/256" }
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let own = c.list_own_worlds().await.unwrap();
    assert_eq!(own.len(), 2);
    assert_eq!(own[1].release_status.as_deref(), Some("private"));

    let merged = merge_worlds(worlds, own);
    assert_eq!(merged.len(), 2);
    assert!(merged[0].favorite && merged[0].own);
    assert!(!merged[1].favorite && merged[1].own);
}

#[tokio::test]
async fn create_instance_and_invites() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    const W: &str = "wrld_12345678-1234-1234-1234-123456789abc";
    const U: &str = "usr_12345678-1234-1234-1234-123456789abc";
    let location = format!("{W}:12345~region(jp)");

    Mock::given(method("POST"))
        .and(path("/api/1/instances"))
        .and(body_json(serde_json::json!({
            "worldId": W, "type": "private", "region": "jp", "ownerId": "usr_me", "canRequestInvite": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": format!("{W}:12345~region(jp)"), "worldId": W, "instanceId": "12345~region(jp)",
            "location": location, "region": "jp", "type": "private"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let req = CreateInstanceRequest::from_ui(W, UiInstanceType::InvitePlus, "jp", "usr_me");
    let inst = c.create_instance(&req).await.unwrap();
    assert_eq!(inst.location, location);

    // 招待 (ボディに instanceId)
    Mock::given(method("POST"))
        .and(path(format!("/api/1/invite/{U}")))
        .and(body_json(serde_json::json!({ "instanceId": location })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "not_1", "receiverUserId": U, "created_at": "2026-01-01T00:00:00.000Z", "type": "invite"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let n = c.invite_user(U, &location).await.unwrap();
    assert_eq!(n.id, "not_1");
    assert_eq!(n.created_at.as_deref(), Some("2026-01-01T00:00:00.000Z"));

    // 自分への招待: パス中の instanceId はエンコードされる
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/1/invite/myself/to/{W}:12345~region%28jp%29"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "not_2" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let n = c.invite_myself(&location).await.unwrap();
    assert_eq!(n.id, "not_2");

    // フレンドでない相手 → 403 Api エラー
    Mock::given(method("POST"))
        .and(path(
            "/api/1/invite/usr_00000000-0000-0000-0000-000000000000",
        ))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": { "message": "You are not friends with this user", "status_code": 403 }
        })))
        .mount(&server)
        .await;
    let err = c
        .invite_user("usr_00000000-0000-0000-0000-000000000000", &location)
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(403));
    assert_eq!(err.to_string(), "(403) You are not friends with this user");

    // 入力バリデーション (サーバーには届かない)
    assert!(matches!(
        c.invite_user("bob", &location).await.unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        c.invite_myself(W).await.unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        c.get_world("wrld_nope").await.unwrap_err(),
        Error::Validation(_)
    ));
}

#[tokio::test]
async fn logout_and_image_fetch() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("PUT"))
        .and(path("/api/1/logout"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "success": { "message": "Ok!", "status_code": 200 } }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    c.logout().await.unwrap();

    // 画像はホスト許可リスト外 (テストサーバーは vrchat.cloud ではない) なので拒否される
    let err = c
        .fetch_image_data_url(&format!("{}/api/1/image/x/1/256", server.uri()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}
