//! 认证 / 健康检查 / 前端配置 HTTP 编排。

use std::sync::Arc;

use axum::extract::Extension;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use subtle::ConstantTimeEq;

use admin_kit_core::AppState;

use crate::account::{
    account_claims, new_account, normalize_email, validate_password, Account, AccountCrypto,
    AccountError, Role,
};
use crate::auth::SessionAuth;
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::models::{
    ChangePasswordRequest, ConfigResponse, FeaturesResponse, LoginRequest, UserEnvelope, UserView,
};
use crate::session::{clear_session_cookie, session_cookie};
use crate::AppCtx;

/// `GET /health` —— 公开。
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "admin-kit-api",
    }))
}

/// `GET /api/config` —— 仅暴露前端需要的功能开关。
pub async fn config() -> Json<ConfigResponse> {
    Json(ConfigResponse {
        features: FeaturesResponse {
            email_auth: true,
            signup: false,
            admin: true,
        },
    })
}

/// 密码哈希在 blocking 线程执行，避免阻塞 async worker。
async fn hash_password(crypto: Arc<AccountCrypto>, password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || crypto.hash(&password))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
}

async fn verify_password_hash(crypto: Arc<AccountCrypto>, hash: String, password: String) -> bool {
    tokio::task::spawn_blocking(move || crypto.verify(&hash, &password))
        .await
        .unwrap_or(false)
}

/// 部署时写入 SSM 的管理员引导凭据（本地开发走 `ADMIN_KIT_DEV_ADMIN_*`）。
fn configured_admin(state: &AppState) -> Option<(String, String)> {
    let email = state
        .config
        .secret("admin_email")
        .map(str::to_string)
        .or_else(|| std::env::var("ADMIN_KIT_DEV_ADMIN_EMAIL").ok())
        .and_then(|raw| normalize_email(&raw).ok())?;
    let password = state
        .config
        .secret("admin_password")
        .map(str::to_string)
        .or_else(|| std::env::var("ADMIN_KIT_DEV_ADMIN_PASSWORD").ok())
        .filter(|p| !p.is_empty())?;
    Some((email, password))
}

fn cookie_header(cookie: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(cookie).map_err(|e| ApiError::internal(e.to_string()))
}

/// 签发账号会话 Cookie（JSON body 由调用方决定）。
fn attach_session_cookie(
    response: &mut Response,
    ctx: &AppCtx,
    account: &Account,
) -> Result<(), ApiError> {
    let token = ctx
        .state
        .jwt
        .sign(&account_claims(account))
        .map_err(|e| ApiError::internal(e.to_string()))?;
    response
        .headers_mut()
        .append(header::SET_COOKIE, cookie_header(&session_cookie(&token))?);
    Ok(())
}

fn session_response(
    ctx: &AppCtx,
    account: &Account,
    status: StatusCode,
) -> Result<Response, ApiError> {
    let mut response = (
        status,
        Json(UserEnvelope {
            user: UserView::from(account),
        }),
    )
        .into_response();
    attach_session_cookie(&mut response, ctx, account)?;
    Ok(response)
}

/// `POST /api/auth/login` —— 邮箱 + 密码。
///
/// - 失败一律 `401 INVALID_CREDENTIALS`（不区分不存在 / 密码错 / 禁用）；
/// - 管理员引导：邮箱不存在且与 SSM `admin_email`/`admin_password` 匹配时创建 admin；
/// - 已存在的账号（包括 role=user）绝不自动提权。
pub async fn login(
    Extension(ctx): Extension<AppCtx>,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<Response, ApiError> {
    let email = normalize_email(&body.email).map_err(|_| ApiError::invalid_credentials())?;
    if body.password.is_empty() || body.password.chars().count() > 128 {
        return Err(ApiError::invalid_credentials());
    }

    if let Some(account) = ctx.accounts.find_by_email(&email).await? {
        if !account.disabled
            && verify_password_hash(
                ctx.crypto.clone(),
                account.password_hash.clone(),
                body.password.clone(),
            )
            .await
        {
            return session_response(&ctx, &account, StatusCode::OK);
        }
        return Err(ApiError::invalid_credentials());
    }

    // 仅当邮箱不存在时才允许引导管理员创建（已存在但 role=user 不自动提权）。
    if let Some((admin_email, admin_password)) = configured_admin(&ctx.state) {
        if email == admin_email
            && bool::from(admin_password.as_bytes().ct_eq(body.password.as_bytes()))
        {
            let hash = hash_password(ctx.crypto.clone(), body.password.clone()).await?;
            let account = new_account(&email, &hash, Role::Admin, admin_kit_dynamo::unix_ms());
            match ctx.accounts.create(account.clone()).await {
                Ok(()) => {
                    tracing::info!(account = %account.id, "bootstrap admin account created");
                    return session_response(&ctx, &account, StatusCode::OK);
                }
                Err(AccountError::DuplicateEmail) => {
                    tracing::warn!(
                        "bootstrap admin email was registered concurrently; refusing to promote"
                    );
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    Err(ApiError::invalid_credentials())
}

/// `GET /api/auth/me`
pub async fn me(auth: SessionAuth) -> Json<UserEnvelope> {
    Json(UserEnvelope {
        user: UserView::from(&auth.account),
    })
}

/// `POST /api/auth/logout` —— 清除会话 Cookie，204。
pub async fn logout() -> Result<Response, ApiError> {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, cookie_header(&clear_session_cookie())?);
    Ok(response)
}

/// `POST /api/auth/password` —— 当前用户改密；递增 session_version 并重新签发 Cookie。
pub async fn change_password(
    auth: SessionAuth,
    Extension(ctx): Extension<AppCtx>,
    ApiJson(body): ApiJson<ChangePasswordRequest>,
) -> Result<Response, ApiError> {
    let account = &auth.account;
    if !verify_password_hash(
        ctx.crypto.clone(),
        account.password_hash.clone(),
        body.current_password.clone(),
    )
    .await
    {
        return Err(ApiError::unauthorized("invalid current password"));
    }
    validate_password(&body.new_password)?;
    let hash = hash_password(ctx.crypto.clone(), body.new_password.clone()).await?;
    let updated = ctx
        .accounts
        .change_password(&account.owner, &hash, admin_kit_dynamo::unix_ms())
        .await?;
    tracing::info!(account = %updated.id, "account password changed");

    let mut response = StatusCode::NO_CONTENT.into_response();
    attach_session_cookie(&mut response, &ctx, &updated)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AdminUser, UserListResponse};
    use crate::testutil::{json_body, TestApp};

    async fn call(
        app: &TestApp,
        method: &str,
        uri: &str,
        cookie: Option<&str>,
        body: Option<&str>,
    ) -> Response {
        app.call(method, uri, cookie, body).await
    }

    #[tokio::test]
    async fn health_and_config_are_public() {
        let app = TestApp::new(Default::default());

        let res = call(&app, "GET", "/health", None, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_body(res).await;
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], "admin-kit-api");

        let res = call(&app, "GET", "/api/config", None, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_body(res).await;
        assert_eq!(json["features"]["emailAuth"], true);
        assert_eq!(json["features"]["signup"], false);
        assert_eq!(json["features"]["admin"], true);
    }

    #[tokio::test]
    async fn login_success_sets_cookie_and_me_returns_user() {
        let app = TestApp::new(Default::default());
        let account = app
            .seed("alice@example.com", "password123", Role::User)
            .await;

        let res = call(
            &app,
            "POST",
            "/api/auth/login",
            None,
            Some(r#"{"email":"Alice@Example.com ","password":"password123"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let json = json_body(res).await;
        assert_eq!(json["user"]["id"], account.id);
        assert_eq!(json["user"]["email"], "alice@example.com");
        assert_eq!(json["user"]["role"], "user");
        assert_eq!(json["user"]["disabled"], false);
        assert!(json.get("password_hash").is_none());

        let res = call(&app, "GET", "/api/auth/me", Some(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_body(res).await;
        assert_eq!(json["user"]["id"], account.id);
    }

    #[tokio::test]
    async fn login_failures_are_unified_401() {
        let app = TestApp::new(Default::default());
        let disabled = app.seed("off@example.com", "password123", Role::User).await;
        app.ctx
            .accounts
            .update(
                &disabled.owner,
                crate::account_store::AccountPatch {
                    role: None,
                    disabled: Some(true),
                },
                admin_kit_dynamo::unix_ms(),
            )
            .await
            .unwrap();
        app.seed("on@example.com", "password123", Role::User).await;

        for body in [
            r#"{"email":"on@example.com","password":"wrong-password"}"#,
            r#"{"email":"missing@example.com","password":"password123"}"#,
            r#"{"email":"off@example.com","password":"password123"}"#,
            r#"{"email":"not-an-email","password":"password123"}"#,
        ] {
            let res = call(&app, "POST", "/api/auth/login", None, Some(body)).await;
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "body={body}");
            let json = json_body(res).await;
            assert_eq!(json["error"]["code"], "INVALID_CREDENTIALS");
        }
    }

    #[tokio::test]
    async fn admin_bootstrap_creates_admin_only_when_absent() {
        let mut secrets = std::collections::HashMap::new();
        secrets.insert("admin_email".into(), "boss@example.com".into());
        secrets.insert("admin_password".into(), "bootstrap-pass".into());
        let app = TestApp::new(secrets);

        let res = call(
            &app,
            "POST",
            "/api/auth/login",
            None,
            Some(r#"{"email":"boss@example.com","password":"bootstrap-pass"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_body(res).await;
        assert_eq!(json["user"]["role"], "admin");

        // 管理员引导密码不可复用于已有普通账号。
        let mut secrets = std::collections::HashMap::new();
        secrets.insert("admin_email".into(), "member@example.com".into());
        secrets.insert("admin_password".into(), "bootstrap-pass".into());
        let app = TestApp::new(secrets);
        app.seed("member@example.com", "member-pass", Role::User)
            .await;

        let res = call(
            &app,
            "POST",
            "/api/auth/login",
            None,
            Some(r#"{"email":"member@example.com","password":"bootstrap-pass"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let json = json_body(res).await;
        assert_eq!(json["error"]["code"], "INVALID_CREDENTIALS");

        let target = app
            .ctx
            .accounts
            .find_by_email("member@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(target.role, "user");
    }

    #[tokio::test]
    async fn logout_clears_cookie() {
        let app = TestApp::new(Default::default());
        let res = call(&app, "POST", "/api/auth/logout", None, None).await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.starts_with("admin_session=;"));
        assert!(cookie.contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn change_password_rotates_session_and_cookie() {
        let app = TestApp::new(Default::default());
        let account = app.seed("a@example.com", "password123", Role::User).await;
        let old_cookie = app.cookie(&account);

        let res = call(
            &app,
            "POST",
            "/api/auth/password",
            Some(&old_cookie),
            Some(r#"{"current_password":"password123","new_password":"new-password"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let new_cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        assert_ne!(new_cookie, old_cookie);

        // 旧 Cookie 因 session_version 递增而立即失效。
        let res = call(&app, "GET", "/api/auth/me", Some(&old_cookie), None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 新 cookie 可用；新密码可登录。
        let res = call(&app, "GET", "/api/auth/me", Some(&new_cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let res = call(
            &app,
            "POST",
            "/api/auth/login",
            None,
            Some(r#"{"email":"a@example.com","password":"new-password"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn change_password_rejects_wrong_current_password() {
        let app = TestApp::new(Default::default());
        let account = app.seed("a@example.com", "password123", Role::User).await;
        let res = call(
            &app,
            "POST",
            "/api/auth/password",
            Some(&app.cookie(&account)),
            Some(r#"{"current_password":"wrong-pass","new_password":"new-password"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn change_password_rejects_short_new_password() {
        let app = TestApp::new(Default::default());
        let account = app.seed("a@example.com", "password123", Role::User).await;
        let res = call(
            &app,
            "POST",
            "/api/auth/password",
            Some(&app.cookie(&account)),
            Some(r#"{"current_password":"password123","new_password":"short"}"#),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let json = json_body(res).await;
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
    }

    #[tokio::test]
    async fn me_requires_session() {
        let app = TestApp::new(Default::default());
        let res = call(&app, "GET", "/api/auth/me", None, None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn response_types_are_serializable() {
        let _ = serde_json::to_value(UserListResponse {
            items: Vec::<AdminUser>::new(),
            next_cursor: None,
        })
        .unwrap();
    }
}
