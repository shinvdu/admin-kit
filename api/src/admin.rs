//! 管理后台：用户列表 / 创建 / 改角色 / 禁用 / 重置密码 / 统计。
//!
//! 所有路由仅接受 Cookie 会话且 role ∈ {admin, owner}（API Key 完全不可用）。

use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};

use crate::account::{
    new_account, normalize_email, owner_from_account_id, validate_password, AccountCrypto,
    AccountListOpts, AccountStatus, Role,
};
use crate::account_store::AccountPatch;
use crate::auth::AdminAuth;
use crate::error::ApiError;
use crate::extract::{ApiJson, ApiQuery};
use crate::models::{
    AdminListQuery, AdminUser, CreateUserRequest, StatsResponse, UpdateUserRequest,
    UserListResponse,
};
use crate::AppCtx;

/// 管理后台路由。
pub fn routes() -> Router {
    Router::new()
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/users/{id}", patch(update_user))
        .route("/api/admin/stats", get(stats))
}

async fn hash_password(crypto: Arc<AccountCrypto>, password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || crypto.hash(&password))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
}

fn parse_create_role(raw: Option<&str>) -> Result<Role, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Role::User),
        Some("admin") => Ok(Role::Admin),
        Some("user") => Ok(Role::User),
        Some(_) => Err(ApiError::bad_request("invalid role")),
    }
}

fn parse_update_role(raw: &str) -> Result<Role, ApiError> {
    match raw.trim() {
        "admin" => Ok(Role::Admin),
        "user" => Ok(Role::User),
        _ => Err(ApiError::bad_request("invalid role")),
    }
}

fn parse_status_filter(raw: Option<&str>) -> Result<Option<AccountStatus>, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some("active") => Ok(Some(AccountStatus::Active)),
        Some("disabled") => Ok(Some(AccountStatus::Disabled)),
        Some(_) => Err(ApiError::bad_request("invalid status filter")),
    }
}

fn parse_role_filter(raw: Option<&str>) -> Result<Option<Role>, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some("owner") => Ok(Some(Role::Owner)),
        Some("admin") => Ok(Some(Role::Admin)),
        Some("user") => Ok(Some(Role::User)),
        Some(_) => Err(ApiError::bad_request("invalid role filter")),
    }
}

/// `GET /api/admin/users`
pub async fn list_users(
    _admin: AdminAuth,
    Extension(ctx): Extension<AppCtx>,
    ApiQuery(query): ApiQuery<AdminListQuery>,
) -> Result<Json<UserListResponse>, ApiError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let role = parse_role_filter(query.role.as_deref())?;
    let status = parse_status_filter(query.status.as_deref())?;
    let page = ctx
        .accounts
        .list(AccountListOpts {
            limit,
            cursor: query.cursor,
            q: query.q,
            role,
            status,
        })
        .await?;
    Ok(Json(UserListResponse {
        items: page.items.into_iter().map(AdminUser::from).collect(),
        next_cursor: page.next_cursor,
    }))
}

/// `POST /api/admin/users`
pub async fn create_user(
    _admin: AdminAuth,
    Extension(ctx): Extension<AppCtx>,
    ApiJson(body): ApiJson<CreateUserRequest>,
) -> Result<(StatusCode, Json<AdminUser>), ApiError> {
    let email = normalize_email(&body.email)?;
    validate_password(&body.password)?;
    let role = parse_create_role(body.role.as_deref())?;
    let hash = hash_password(ctx.crypto.clone(), body.password.clone()).await?;
    let account = new_account(&email, &hash, role, admin_kit_dynamo::unix_ms());
    ctx.accounts.create(account.clone()).await?;
    tracing::info!(account = %account.id, role = %account.role, "admin created account");
    Ok((StatusCode::CREATED, Json(AdminUser::from(&account))))
}

/// `PATCH /api/admin/users/{id}`
pub async fn update_user(
    admin: AdminAuth,
    Extension(ctx): Extension<AppCtx>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<UpdateUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let owner = owner_from_account_id(&id)?;
    let target = ctx
        .accounts
        .get(&owner)
        .await?
        .ok_or_else(|| ApiError::not_found("account not found"))?;

    let is_self = admin.session.account.owner == owner;
    let new_role = match body.role.as_deref() {
        Some(raw) => Some(parse_update_role(raw)?),
        None => None,
    };
    let new_disabled = body.disabled;

    if is_self {
        if new_role.is_some_and(|role| role.as_str() != target.role) {
            return Err(ApiError::self_action("cannot change your own role"));
        }
        if new_disabled == Some(true) {
            return Err(ApiError::self_action("cannot disable yourself"));
        }
        if body.password.is_some() {
            return Err(ApiError::self_action(
                "use /api/auth/password to change your own password",
            ));
        }
    }

    // 不能移除最后一个有效管理员（并发下为尽力而为）。
    let removes_admin = target.role == "admin"
        && !target.disabled
        && (new_role.is_some_and(|role| !role.can_admin()) || new_disabled == Some(true));
    if removes_admin && ctx.accounts.count_active_admins().await? <= 1 {
        return Err(ApiError::last_admin("cannot remove the last active admin"));
    }

    let now = admin_kit_dynamo::unix_ms();
    let mut current = target;
    if let Some(password) = body.password {
        validate_password(&password)?;
        let hash = hash_password(ctx.crypto.clone(), password).await?;
        current = ctx.accounts.change_password(&owner, &hash, now).await?;
        tracing::info!(account = %current.id, "admin reset account password");
    }

    let updated = ctx
        .accounts
        .update(
            &owner,
            AccountPatch {
                role: new_role,
                disabled: new_disabled,
            },
            now,
        )
        .await?;
    if updated.disabled != current.disabled || updated.role != current.role {
        tracing::info!(
            account = %updated.id,
            role = %updated.role,
            disabled = updated.disabled,
            "admin updated account"
        );
    }
    Ok(Json(AdminUser::from(&updated)))
}

/// `GET /api/admin/stats`
pub async fn stats(
    _admin: AdminAuth,
    Extension(ctx): Extension<AppCtx>,
) -> Result<Json<StatsResponse>, ApiError> {
    Ok(Json(StatsResponse::from(ctx.accounts.stats().await?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{json_body, TestApp};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn admin_cookie(app: &TestApp, account: &crate::account::Account) -> String {
        app.cookie(account)
    }

    #[tokio::test]
    async fn admin_authorization_matrix() {
        let app = TestApp::new(Default::default());
        let user = app.seed("u@example.com", "password123", Role::User).await;
        let admin = app.seed("a@example.com", "password123", Role::Admin).await;

        assert_eq!(
            app.call("GET", "/api/admin/users", None, None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.call(
                "GET",
                "/api/admin/users",
                Some(&admin_cookie(&app, &user)),
                None
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        // API Key 不是会话，直接 401。
        let key_res = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/users")
                    .header("x-api-key", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(key_res.status(), StatusCode::UNAUTHORIZED);

        let res = app
            .call(
                "GET",
                "/api/admin/users",
                Some(&admin_cookie(&app, &admin)),
                None,
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_user_and_reject_duplicates_and_bad_input() {
        let app = TestApp::new(Default::default());
        let admin = app
            .seed("admin@example.com", "password123", Role::Admin)
            .await;
        let cookie = admin_cookie(&app, &admin);

        let res = app
            .call(
                "POST",
                "/api/admin/users",
                Some(&cookie),
                Some(r#"{"email":"New@Example.com","password":"password123","role":"user"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let json = json_body(res).await;
        assert_eq!(json["email"], "new@example.com");
        assert_eq!(json["role"], "user");
        assert_eq!(json["disabled"], false);
        assert!(json["created_at"].as_u64().is_some());
        assert!(json.get("password_hash").is_none());

        let res = app
            .call(
                "POST",
                "/api/admin/users",
                Some(&cookie),
                Some(r#"{"email":"new@example.com","password":"password123"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        assert_eq!(json_body(res).await["error"]["code"], "EMAIL_TAKEN");

        for body in [
            r#"{"email":"x@example.com","password":"password123","role":"owner"}"#,
            r#"{"email":"not-an-email","password":"password123"}"#,
            r#"{"email":"x@example.com","password":"short"}"#,
        ] {
            let res = app
                .call("POST", "/api/admin/users", Some(&cookie), Some(body))
                .await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "body={body}");
            assert_eq!(json_body(res).await["error"]["code"], "BAD_REQUEST");
        }
    }

    #[tokio::test]
    async fn list_pagination_and_filters() {
        let app = TestApp::new(Default::default());
        let admin = app
            .seed_at("admin@example.com", "password123", Role::Admin, 100)
            .await;
        let a = app
            .seed_at("alice@example.com", "password123", Role::User, 200)
            .await;
        let b = app
            .seed_at("bob@example.com", "password123", Role::User, 300)
            .await;
        app.seed_at("carol@example.com", "password123", Role::User, 400)
            .await;
        let cookie = admin_cookie(&app, &admin);

        let res = app
            .call(
                "GET",
                "/api/admin/users?limit=2&role=user",
                Some(&cookie),
                None,
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_body(res).await;
        assert_eq!(json["items"].as_array().unwrap().len(), 2);
        assert_eq!(json["items"][0]["email"], "carol@example.com");
        assert_eq!(json["items"][1]["email"], "bob@example.com");
        let cursor = json["next_cursor"].as_str().unwrap().to_string();

        let res = app
            .call(
                "GET",
                &format!("/api/admin/users?limit=2&role=user&cursor={cursor}"),
                Some(&cookie),
                None,
            )
            .await;
        let json = json_body(res).await;
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["id"], a.id);
        assert!(json["next_cursor"].is_null());

        let res = app
            .call(
                "GET",
                "/api/admin/users?q=ALI&role=user",
                Some(&cookie),
                None,
            )
            .await;
        let json = json_body(res).await;
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["id"], a.id);

        app.ctx
            .accounts
            .update(
                &b.owner,
                AccountPatch {
                    role: None,
                    disabled: Some(true),
                },
                500,
            )
            .await
            .unwrap();
        let res = app
            .call(
                "GET",
                "/api/admin/users?status=disabled&role=user",
                Some(&cookie),
                None,
            )
            .await;
        let json = json_body(res).await;
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["id"], b.id);

        let res = app
            .call("GET", "/api/admin/users?role=owner", Some(&cookie), None)
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(json_body(res).await["items"].as_array().unwrap().is_empty());

        let res = app
            .call("GET", "/api/admin/users?role=root", Some(&cookie), None)
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn cannot_disable_or_demote_self() {
        let app = TestApp::new(Default::default());
        let admin = app
            .seed("admin@example.com", "password123", Role::Admin)
            .await;
        let cookie = admin_cookie(&app, &admin);

        let res = app
            .call(
                "PATCH",
                &format!("/api/admin/users/{}", admin.id),
                Some(&cookie),
                Some(r#"{"disabled":true}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json_body(res).await["error"]["code"], "SELF_ACTION");

        let res = app
            .call(
                "PATCH",
                &format!("/api/admin/users/{}", admin.id),
                Some(&cookie),
                Some(r#"{"role":"user"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json_body(res).await["error"]["code"], "SELF_ACTION");
    }

    #[tokio::test]
    async fn last_active_admin_is_protected() {
        let app = TestApp::new(Default::default());
        // `owner` 角色仅用于展示兼容，不计入 active admin 数。
        let owner = app
            .seed_role_string("owner@example.com", "password123", "owner", 100)
            .await;
        let only_admin = app
            .seed_at("admin@example.com", "password123", Role::Admin, 200)
            .await;
        let cookie = admin_cookie(&app, &owner);

        let res = app
            .call(
                "PATCH",
                &format!("/api/admin/users/{}", only_admin.id),
                Some(&cookie),
                Some(r#"{"role":"user"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json_body(res).await["error"]["code"], "LAST_ADMIN");

        // 再加一个 active admin 后即可降级。
        app.seed_at("admin2@example.com", "password123", Role::Admin, 300)
            .await;
        let res = app
            .call(
                "PATCH",
                &format!("/api/admin/users/{}", only_admin.id),
                Some(&cookie),
                Some(r#"{"role":"user"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json_body(res).await["role"], "user");
    }

    #[tokio::test]
    async fn admin_password_reset_revokes_target_session() {
        let app = TestApp::new(Default::default());
        let admin = app
            .seed("admin@example.com", "password123", Role::Admin)
            .await;
        let target = app.seed("u@example.com", "password123", Role::User).await;
        let admin_cookie = admin_cookie(&app, &admin);
        let target_cookie = app.cookie(&target);

        // 目标当前会话可用。
        assert_eq!(
            app.call("GET", "/api/auth/me", Some(&target_cookie), None)
                .await
                .status(),
            StatusCode::OK
        );

        let res = app
            .call(
                "PATCH",
                &format!("/api/admin/users/{}", target.id),
                Some(&admin_cookie),
                Some(r#"{"password":"new-password"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);

        // session_version +1 → 旧 Cookie 立即失效。
        assert_eq!(
            app.call("GET", "/api/auth/me", Some(&target_cookie), None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let res = app
            .call(
                "POST",
                "/api/auth/login",
                None,
                Some(r#"{"email":"u@example.com","password":"new-password"}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_id_and_missing_user() {
        let app = TestApp::new(Default::default());
        let admin = app
            .seed("admin@example.com", "password123", Role::Admin)
            .await;
        let cookie = admin_cookie(&app, &admin);

        let res = app
            .call(
                "PATCH",
                "/api/admin/users/not-a-uuid",
                Some(&cookie),
                Some(r#"{"disabled":true}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let res = app
            .call(
                "PATCH",
                "/api/admin/users/0192f0c4e6b84a8f9c6e1d2b3a4c5d6e",
                Some(&cookie),
                Some(r#"{"disabled":true}"#),
            )
            .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stats_endpoint_counts_all_users() {
        let app = TestApp::new(Default::default());
        let admin = app
            .seed("admin@example.com", "password123", Role::Admin)
            .await;
        let user = app.seed("u@example.com", "password123", Role::User).await;
        app.seed("off@example.com", "password123", Role::User).await;
        app.ctx
            .accounts
            .update(
                &user.owner,
                AccountPatch {
                    role: None,
                    disabled: Some(true),
                },
                500,
            )
            .await
            .unwrap();

        let res = app
            .call(
                "GET",
                "/api/admin/stats",
                Some(&admin_cookie(&app, &admin)),
                None,
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_body(res).await;
        assert_eq!(json["total_users"], 3);
        assert_eq!(json["active_users"], 2);
        assert_eq!(json["disabled_users"], 1);
    }

    #[test]
    fn role_filter_helpers() {
        assert_eq!(parse_create_role(None).unwrap(), Role::User);
        assert_eq!(parse_create_role(Some("admin")).unwrap(), Role::Admin);
        assert!(parse_create_role(Some("owner")).is_err());
        assert_eq!(parse_update_role("user").unwrap(), Role::User);
        assert!(parse_update_role("root").is_err());
        assert_eq!(parse_role_filter(Some("owner")).unwrap(), Some(Role::Owner));
        assert!(parse_role_filter(Some("root")).is_err());
        assert_eq!(
            parse_status_filter(Some("active")).unwrap(),
            Some(AccountStatus::Active)
        );
        assert!(parse_status_filter(Some("foo")).is_err());
    }
}
