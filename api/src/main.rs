//! admin-kit API 入口（独立后台 Lambda）。
//!
//! 只提供认证与用户管理：
//! - `GET  /health`
//! - `GET  /api/config`
//! - `POST /api/auth/login` / `POST /api/auth/logout` / `GET /api/auth/me`
//! - `POST /api/auth/password`
//! - `GET  /api/admin/users` / `POST /api/admin/users`
//! - `PATCH /api/admin/users/{id}`
//! - `GET  /api/admin/stats`
//!
//! 本地与 Lambda 共用同一份代码：`run_with_setup` 监听 `PORT`，
//! Lambda 由 Web Adapter 转发。

mod account;
mod account_store;
mod admin;
mod auth;
mod dynamo_account_store;
mod error;
mod extract;
mod handlers;
mod memory_account_store;
mod models;
mod session;
#[cfg(test)]
mod testutil;

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Extension};
use axum::routing::{get, post};
use axum::Router;

use admin_kit_core::{run_with_setup, AdminKitRouterExt, AppState};

use crate::account::AccountCrypto;
use crate::account_store::AccountStore;
use crate::dynamo_account_store::DynamoAccountStore;
use crate::memory_account_store::MemoryAccountStore;

pub const DEFAULT_TABLE: &str = "admin-kit-dev-users";

/// 应用上下文：框架状态 + 可替换的账号存储 / 密码哈希参数。
#[derive(Clone)]
pub struct AppCtx {
    pub state: AppState,
    pub accounts: Arc<dyn AccountStore>,
    pub crypto: Arc<AccountCrypto>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_with_setup(|state| async move {
        let accounts = build_account_store(&state);
        tracing::info!(
            store = accounts.kind(),
            table = %table_name(),
            "admin-kit dependencies assembled"
        );
        let ctx = AppCtx {
            state: state.clone(),
            accounts,
            crypto: Arc::new(AccountCrypto::default()),
        };
        Ok(build_router(ctx))
    })
    .await
}

/// 装配路由（测试也走同一入口，保证行为一致）。
pub(crate) fn build_router(ctx: AppCtx) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/api/config", get(handlers::config))
        .route("/api/auth/login", post(handlers::login))
        .route("/api/auth/password", post(handlers::change_password))
        .route("/api/auth/logout", post(handlers::logout))
        .route("/api/auth/me", get(handlers::me))
        .merge(admin::routes())
        // 请求体上限 256KB
        .layer(DefaultBodyLimit::max(256 * 1024))
        // AppCtx 通过 Extension 注入（axum 0.8 serve 只接受 Router<()>）
        .layer(Extension(ctx))
        .with_admin_kit_defaults()
}

/// `ADMIN_KIT_TABLE` 默认 `admin-kit-dev-users`。
fn table_name() -> String {
    std::env::var("ADMIN_KIT_TABLE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_TABLE.to_string())
}

/// `ADMIN_KIT_STORE=memory` 用于本地/测试；默认 DynamoDB。
fn build_account_store(state: &AppState) -> Arc<dyn AccountStore> {
    match std::env::var("ADMIN_KIT_STORE").ok().as_deref() {
        Some("memory") => {
            tracing::info!("using in-memory account store");
            Arc::new(MemoryAccountStore::new())
        }
        _ => {
            let store = DynamoAccountStore::new(&state.aws_config, table_name());
            tracing::info!(table = store.table_name(), "using DynamoDB account store");
            Arc::new(store)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestApp;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn router_mounts_core_routes() {
        let app = TestApp::new(Default::default());
        assert_eq!(
            app.call("GET", "/health", None, None).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            app.call("GET", "/api/config", None, None).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            app.call("GET", "/api/auth/me", None, None).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.call("GET", "/api/admin/users", None, None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn default_table_name_constant() {
        assert_eq!(DEFAULT_TABLE, "admin-kit-dev-users");
    }
}
