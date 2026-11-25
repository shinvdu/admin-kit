//! 混合配置策略：环境变量 + SSM 参数存储。
//!
//! - 非敏感配置（项目名、环境、表前缀等）走环境变量注入；
//! - 敏感配置（JWT 密钥、API Key 等）存在 SSM 参数存储，冷启动时**一次**
//!   `GetParametersByPath` 批量拉取全部密钥，缓存在内存里，热请求零开销。
//!
//! 本地开发模式（未设置 `ADMIN_KIT_SECRETS_PATH`）自动降级：密钥从
//! `ADMIN_KIT_DEV_*` 环境变量读取，无需任何 AWS 依赖即可跑通框架。

use std::collections::HashMap;

/// 功能开关。未知/缺省一律按关闭处理；安全能力不受开关控制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureFlags {
    /// GitHub OAuth 是否可用（依据 client id/secret 是否配置推断）。
    pub github_auth: bool,
    /// 共享密码兜底是否可用（依据 SSM `access_password` 是否配置推断）。
    pub password_auth: bool,
    /// 多用户数据隔离（`ADMIN_KIT_ENABLE_MULTI_USER`）。
    pub multi_user: bool,
    /// 邮箱密码账号登录（`ADMIN_KIT_ENABLE_EMAIL_AUTH`）。
    pub email_auth: bool,
    /// 自助注册（`ADMIN_KIT_ENABLE_SIGNUP`；仅 email_auth 开启时生效）。
    pub signup: bool,
    /// 管理后台（由 email_auth 派生）。
    pub admin: bool,
}

impl FeatureFlags {
    /// 自托管安全默认：除核心能力外全部关闭。
    pub fn self_host_defaults() -> Self {
        Self::default()
    }

    /// 从环境变量解析。测试不修改进程环境，直接构造结构体注入。
    pub fn from_env(secrets: &HashMap<String, String>) -> Self {
        let email_auth = parse_bool(std::env::var("ADMIN_KIT_ENABLE_EMAIL_AUTH").ok().as_deref());
        let env_non_empty = |key: &str| {
            std::env::var(key)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        };
        let secret_non_empty = |key: &str| {
            secrets
                .get(key)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        };
        Self {
            github_auth: (secret_non_empty("github_client_id")
                && secret_non_empty("github_client_secret"))
                || (env_non_empty("ADMIN_KIT_GITHUB_CLIENT_ID")
                    && env_non_empty("ADMIN_KIT_GITHUB_CLIENT_SECRET")),
            password_auth: secret_non_empty("access_password")
                || env_non_empty("ADMIN_KIT_DEV_ACCESS_PASSWORD"),
            multi_user: parse_bool(std::env::var("ADMIN_KIT_ENABLE_MULTI_USER").ok().as_deref()),
            email_auth,
            // 原始开关；是否真正可用还要叠加 email_auth（见路由注册与 /api/config）。
            signup: parse_bool(std::env::var("ADMIN_KIT_ENABLE_SIGNUP").ok().as_deref()),
            admin: email_auth,
        }
    }
}

/// `true/1/yes/on`（大小写不敏感）视为开启；其余（含缺省）关闭。
pub fn parse_bool(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("true" | "1" | "yes" | "on")
    )
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub project: String,
    pub environment: String,
    pub region: String,
    pub table_prefix: String,
    /// 从 SSM 批量拉取的密钥，key 为参数名最后一段（如 `jwt_seed`）。
    pub secrets: HashMap<String, String>,
    /// 是否本地开发模式（无 SSM）。
    pub dev_mode: bool,
    /// 功能开关（`GET /api/config` 与路由注册共用）。
    pub features: FeatureFlags,
}

impl AppConfig {
    /// 取一个密钥；SSM 里不存在时返回 None。
    pub fn secret(&self, key: &str) -> Option<&str> {
        self.secrets.get(key).map(|s| s.as_str())
    }
}

pub struct ConfigLoader;

impl ConfigLoader {
    /// 加载配置。SSM 参数路径形如 `/admin-kit/dev/`，参数名取最后一段。
    pub async fn load(aws_config: &aws_config::SdkConfig) -> anyhow::Result<AppConfig> {
        let project = std::env::var("ADMIN_KIT_PROJECT").unwrap_or_else(|_| "admin-kit".into());
        let environment = std::env::var("ADMIN_KIT_ENV").unwrap_or_else(|_| "dev".into());
        let region = std::env::var("AWS_REGION")
            .ok()
            .or_else(|| aws_config.region().map(|r| r.as_ref().to_string()))
            .unwrap_or_else(|| "us-west-2".into());
        let table_prefix = std::env::var("ADMIN_KIT_TABLE_PREFIX")
            .unwrap_or_else(|_| format!("{project}-{environment}-"));

        // 敏感配置：SSM 批量拉取（一次调用拿全部，冷启动缓存）
        let mut secrets = HashMap::new();
        let secrets_path = std::env::var("ADMIN_KIT_SECRETS_PATH").ok();
        if let Some(path) = &secrets_path {
            let client = aws_sdk_ssm::Client::new(aws_config);
            let mut next_token: Option<String> = None;
            loop {
                let out = client
                    .get_parameters_by_path()
                    .path(path)
                    .recursive(true)
                    .with_decryption(true)
                    .set_next_token(next_token.clone())
                    .send()
                    .await?;
                for p in out.parameters() {
                    let key = p
                        .name()
                        .unwrap_or("")
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let value = p.value().unwrap_or("").to_string();
                    if !key.is_empty() {
                        secrets.insert(key, value);
                    }
                }
                next_token = out.next_token().map(|t| t.to_string());
                if next_token.is_none() {
                    break;
                }
            }
            tracing::info!(
                path = %path,
                count = secrets.len(),
                "loaded secrets from SSM (single GetParametersByPath)"
            );
        }

        let features = FeatureFlags::from_env(&secrets);
        tracing::info!(
            github_auth = features.github_auth,
            password_auth = features.password_auth,
            multi_user = features.multi_user,
            email_auth = features.email_auth,
            signup = features.signup,
            admin = features.admin,
            "feature flags resolved"
        );
        Ok(AppConfig {
            project,
            environment,
            region,
            table_prefix,
            secrets,
            dev_mode: secrets_path.is_none(),
            features,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AppConfig {
        AppConfig {
            project: "admin-kit".into(),
            environment: "dev".into(),
            region: "us-west-2".into(),
            table_prefix: "admin-kit-dev-".into(),
            secrets: HashMap::new(),
            dev_mode: true,
            features: FeatureFlags::self_host_defaults(),
        }
    }

    #[test]
    fn bool_parsing_is_strict() {
        assert!(parse_bool(Some("true")));
        assert!(parse_bool(Some("1")));
        assert!(parse_bool(Some(" YES ")));
        assert!(parse_bool(Some("on")));
        assert!(!parse_bool(Some("false")));
        assert!(!parse_bool(Some("0")));
        assert!(!parse_bool(Some("")));
        assert!(!parse_bool(None));
    }

    #[test]
    fn self_host_defaults_close_optional_features() {
        let flags = FeatureFlags::self_host_defaults();
        assert!(!flags.email_auth);
        assert!(!flags.signup);
        assert!(!flags.admin);
        assert!(!flags.multi_user);
    }

    #[test]
    fn secret_lookup() {
        let mut c = config();
        c.secrets.insert("api_key".into(), "k".into());
        assert_eq!(c.secret("api_key"), Some("k"));
        assert_eq!(c.secret("missing"), None);
    }
}
