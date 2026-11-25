//! admin-kit core —— 运行时核心（精简自 operon-core）。
//!
//! 保留：混合配置（env + SSM）、Ed25519 JWT、API Key、统一错误、默认中间件、
//! Lambda/本地一体化启动。删除：OIDC（openidconnect）、SQS、AES-GCM 等不需要的能力。

pub mod auth;
pub mod config;
pub mod error;
pub mod server;

pub use auth::{resolve_jwt_seed, unix_now, ApiKeyAuth, Jwt, JwtAuth, JwtClaims};
pub use config::{AppConfig, ConfigLoader, FeatureFlags};
pub use error::AppError;
pub use server::run_with_setup;

use axum::Router;

/// 应用状态：框架注入，业务代码在 handler 里取用。
#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub jwt: Jwt,
    pub aws_config: aws_config::SdkConfig,
}

// 让 `Jwt` / `AppConfig` / `SdkConfig` 都可以通过 axum 的 State 提取器按需取出。
impl axum::extract::FromRef<AppState> for Jwt {
    fn from_ref(state: &AppState) -> Self {
        state.jwt.clone()
    }
}
impl axum::extract::FromRef<AppState> for AppConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}
impl axum::extract::FromRef<AppState> for aws_config::SdkConfig {
    fn from_ref(state: &AppState) -> Self {
        state.aws_config.clone()
    }
}

/// 默认中间件层：request-id + tracing + CORS。
pub trait AdminKitRouterExt {
    fn with_admin_kit_defaults(self) -> Self;
}

impl<S> AdminKitRouterExt for Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn with_admin_kit_defaults(self) -> Self {
        use axum::http::HeaderName;
        use tower_http::cors::CorsLayer;
        use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
        use tower_http::trace::TraceLayer;

        self.layer(TraceLayer::new_for_http())
            .layer(SetRequestIdLayer::new(
                HeaderName::from_static("x-request-id"),
                MakeRequestUuid,
            ))
            .layer(CorsLayer::permissive())
    }
}

/// 用户代码的一站式导入：`use admin_kit_core::prelude::*;`
pub mod prelude {
    pub use crate::{
        resolve_jwt_seed, run_with_setup, unix_now, AdminKitRouterExt, ApiKeyAuth, AppConfig,
        AppError, AppState, FeatureFlags, Jwt, JwtAuth, JwtClaims,
    };
}
