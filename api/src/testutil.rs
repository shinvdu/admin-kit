//! 测试辅助：离线 AppState / MemoryAccountStore / 全量路由 / 会话 Cookie。

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use tower::ServiceExt;

use admin_kit_core::{AppConfig, AppState, FeatureFlags, Jwt};

use crate::account::{new_account, Account, AccountCrypto, Role};
use crate::account_store::AccountStore;
use crate::memory_account_store::MemoryAccountStore;
use crate::{build_router, AppCtx};

/// 离线构造 AppState：不加载凭证链、不访问 IMDS/网络。
pub fn base_state(secrets: HashMap<String, String>) -> AppState {
    let config = AppConfig {
        project: "admin-kit".into(),
        environment: "test".into(),
        region: "us-west-2".into(),
        table_prefix: "admin-kit-test-".into(),
        secrets,
        dev_mode: true,
        features: FeatureFlags::self_host_defaults(),
    };
    let aws_config = aws_config::SdkConfig::builder()
        .region(aws_config::Region::new("us-west-2"))
        .behavior_version(aws_config::BehaviorVersion::latest())
        .build();
    AppState {
        config,
        jwt: Jwt::from_seed(&[7u8; 32]).expect("seed"),
        aws_config,
    }
}

pub struct TestApp {
    pub router: Router,
    pub ctx: AppCtx,
    pub accounts: Arc<MemoryAccountStore>,
}

impl TestApp {
    pub fn new(mut secrets: HashMap<String, String>) -> Self {
        // 占位空串阻止 configured_admin() 回退到进程环境变量，保证测试完全隔离。
        secrets.entry("admin_email".into()).or_default();
        secrets.entry("admin_password".into()).or_default();
        let state = base_state(secrets);
        let accounts = Arc::new(MemoryAccountStore::new());
        let ctx = AppCtx {
            state,
            accounts: accounts.clone(),
            crypto: Arc::new(AccountCrypto::test()),
        };
        let router = build_router(ctx.clone());
        Self {
            router,
            ctx,
            accounts,
        }
    }

    pub fn empty() -> Self {
        Self::new(HashMap::new())
    }

    pub async fn seed_at(
        &self,
        email: &str,
        password: &str,
        role: Role,
        created_ms: u64,
    ) -> Account {
        let hash = self.ctx.crypto.hash(password).expect("hash");
        let account = new_account(email, &hash, role, created_ms);
        self.accounts.create(account.clone()).await.expect("seed");
        account
    }

    pub async fn seed(&self, email: &str, password: &str, role: Role) -> Account {
        self.seed_at(email, password, role, 1_000).await
    }

    /// 允许 `owner` 这类仅展示兼容的角色写入数据库（本服务不会创建）。
    pub async fn seed_role_string(
        &self,
        email: &str,
        password: &str,
        role: &str,
        created_ms: u64,
    ) -> Account {
        let hash = self.ctx.crypto.hash(password).expect("hash");
        let mut account = new_account(
            email,
            &hash,
            Role::parse(role).unwrap_or(Role::User),
            created_ms,
        );
        account.role = role.to_string();
        self.accounts.create(account.clone()).await.expect("seed");
        account
    }

    pub fn token(&self, account: &Account) -> String {
        self.ctx
            .state
            .jwt
            .sign(&crate::account::account_claims(account))
            .expect("sign session")
    }

    pub fn cookie(&self, account: &Account) -> String {
        format!("admin_session={}", self.token(account))
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        cookie: Option<&str>,
        body: Option<&str>,
    ) -> Response {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header("cookie", cookie);
        }
        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        self.router
            .clone()
            .oneshot(
                builder
                    .body(Body::from(body.unwrap_or("").to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
}

pub async fn json_body(res: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[allow(dead_code)]
pub async fn expect_status(res: Response, status: StatusCode) {
    assert_eq!(res.status(), status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn harness_wires_memory_store_and_router() {
        let app = TestApp::empty();
        let account = app.seed("a@b.com", "password123", Role::User).await;
        assert_eq!(
            app.accounts.get(&account.owner).await.unwrap(),
            Some(account.clone())
        );
        let res = app
            .call("GET", "/api/auth/me", Some(&app.cookie(&account)), None)
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn base_state_is_offline() {
        let state = base_state(HashMap::new());
        assert!(state.config.dev_mode);
        assert_eq!(state.config.project, "admin-kit");
    }
}
