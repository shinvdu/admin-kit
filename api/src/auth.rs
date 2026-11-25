//! 会话提取器：仅接受 HttpOnly Cookie `admin_session`（不接受 API Key）。
//!
//! 每次请求都按 `user:<uuid>` 做一次强一致账号读取：
//! 账号存在、未禁用、`sv == session_version`，角色/邮箱以数据库为准。

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::account::{is_admin_role, Account};
use crate::error::ApiError;
use crate::session::{read_cookie, SESSION_COOKIE};
use crate::AppCtx;

/// 已通过 JWT + 数据库校验的账号会话。
#[derive(Debug, Clone)]
pub struct SessionAuth {
    /// 数据库中的最新账号记录（角色 / 禁用状态 / session_version 均以此为准）。
    pub account: Account,
}

impl FromRequestParts<()> for SessionAuth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &()) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<AppCtx>()
            .cloned()
            .ok_or_else(|| ApiError::internal("app context missing"))?;

        let token = read_cookie(&parts.headers, SESSION_COOKIE)
            .ok_or_else(|| ApiError::unauthorized("missing session cookie"))?;
        let claims = ctx
            .state
            .jwt
            .verify(&token)
            .map_err(|_| ApiError::unauthorized("invalid session"))?;

        if !claims.sub.starts_with("user:") {
            return Err(ApiError::unauthorized("invalid account session"));
        }
        let claimed_sv = claims
            .extra
            .get("sv")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ApiError::unauthorized("invalid account session"))?;

        let account = ctx
            .accounts
            .get(&claims.sub)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::unauthorized("account not found"))?;
        if account.disabled {
            return Err(ApiError::unauthorized("account disabled"));
        }
        if account.session_version != claimed_sv {
            return Err(ApiError::unauthorized("account session is no longer valid"));
        }

        Ok(Self { account })
    }
}

/// 后台管理权限：Cookie 会话 + role ∈ {admin, owner}。
#[derive(Debug, Clone)]
pub struct AdminAuth {
    pub session: SessionAuth,
}

impl FromRequestParts<()> for AdminAuth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &()) -> Result<Self, Self::Rejection> {
        let session = SessionAuth::from_request_parts(parts, state).await?;
        if !is_admin_role(&session.account.role) {
            return Err(ApiError::forbidden("admin role required"));
        }
        Ok(Self { session })
    }
}

#[cfg(test)]
mod tests {
    use crate::account::{account_claims, new_account, AccountCrypto, Role};
    use crate::testutil::{json_body, TestApp};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn me_status(app: &TestApp, cookie: Option<&str>) -> StatusCode {
        let mut builder = Request::builder().uri("/api/auth/me");
        if let Some(cookie) = cookie {
            builder = builder.header("cookie", cookie);
        }
        app.router
            .clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn missing_session_is_401() {
        let app = TestApp::new(Default::default());
        assert_eq!(me_status(&app, None).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_and_tampered_cookie() {
        let app = TestApp::new(Default::default());
        let account = app.seed("a@b.com", "password123", Role::User).await;
        assert_eq!(
            me_status(&app, Some(&app.cookie(&account))).await,
            StatusCode::OK
        );
        assert_eq!(
            me_status(&app, Some("admin_session=not.a.jwt")).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn stale_session_version_is_401() {
        let app = TestApp::new(Default::default());
        let account = app.seed("a@b.com", "password123", Role::User).await;
        let cookie = app.cookie(&account);
        app.ctx
            .accounts
            .change_password(
                &account.owner,
                &AccountCrypto::test().hash("other-password").unwrap(),
                admin_kit_dynamo::unix_ms(),
            )
            .await
            .unwrap();
        assert_eq!(
            app.router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/auth/me")
                        .header("cookie", cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn disabled_account_is_401_even_with_valid_jwt() {
        let app = TestApp::new(Default::default());
        let account = app.seed("a@b.com", "password123", Role::User).await;
        let cookie = app.cookie(&account);
        app.ctx
            .accounts
            .update(
                &account.owner,
                crate::account_store::AccountPatch {
                    role: None,
                    disabled: Some(true),
                },
                admin_kit_dynamo::unix_ms(),
            )
            .await
            .unwrap();
        assert_eq!(
            me_status(&app, Some(&cookie)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn non_admin_gets_403_on_admin_route() {
        let app = TestApp::new(Default::default());
        let user = app.seed("u@b.com", "password123", Role::User).await;
        let res = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/users")
                    .header("cookie", app.cookie(&user))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let json = json_body(res).await;
        assert_eq!(json["error"]["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn non_account_subject_is_rejected() {
        let app = TestApp::new(Default::default());
        let claims = admin_kit_core::JwtClaims {
            sub: "owner".into(),
            email: None,
            iat: admin_kit_core::unix_now(),
            exp: admin_kit_core::unix_now() + 60,
            extra: serde_json::Map::new(),
        };
        let token = app.ctx.state.jwt.sign(&claims).unwrap();
        assert_eq!(
            me_status(&app, Some(&format!("admin_session={token}"))).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn account_claims_shape() {
        let hash = AccountCrypto::test().hash("password123").unwrap();
        let account = new_account("a@b.com", &hash, Role::Admin, 1_000);
        let claims = account_claims(&account);
        assert_eq!(claims.sub, account.owner);
        assert_eq!(claims.email.as_deref(), Some("a@b.com"));
        assert_eq!(claims.extra.get("role").unwrap(), "admin");
        assert_eq!(claims.extra.get("sv").unwrap(), 1);
    }
}
