//! Ed25519 JWT 认证 + API Key 认证。
//!
//! 极简自实现（RFC 7519 + RFC 8037）：`base64url(header).base64url(payload).signature`，
//! 不依赖重型 JWT crate，密码学路径（签名/验证/过期检查）全部显式可控。
//!
//! 业务 JWT 走 `X-Authorization`（CloudFront OAC 会占用标准 `Authorization` 头）。

use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::{Extension, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::AppState;

/// JWT 声明。`sub` 是我们自己的标识（MVP 固定 `owner`），与第三方 provider ID 解耦。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub iat: u64,
    pub exp: u64,
    /// 业务自定义声明（如 `role` / `login`）。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl JwtClaims {
    pub fn new(sub: impl Into<String>) -> Self {
        let now = unix_now();
        Self {
            sub: sub.into(),
            email: None,
            iat: now,
            exp: now + 24 * 3600, // 24 小时
            extra: Default::default(),
        }
    }
}

/// 自实现 JWT 签名/验证器（Ed25519）。
#[derive(Clone)]
pub struct Jwt {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl Jwt {
    pub fn from_seed(seed: &[u8]) -> anyhow::Result<Self> {
        let bytes: &[u8; 32] = seed
            .try_into()
            .map_err(|_| anyhow::anyhow!("ed25519 seed must be exactly 32 bytes"))?;
        let signing_key = SigningKey::from_bytes(bytes);
        let verifying_key = signing_key.verifying_key();
        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// 随机生成一把新密钥（用于脚手架初始化）。
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom_32(&mut seed);
        Self::from_seed(&seed).expect("32 bytes is always valid")
    }

    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    pub fn sign(&self, claims: &JwtClaims) -> anyhow::Result<String> {
        let header = serde_json::json!({ "alg": "EdDSA", "typ": "JWT" });
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
        let signing_input = format!("{header_b64}.{payload_b64}");
        let sig = self.signing_key.sign(signing_input.as_bytes());
        Ok(format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(sig.to_bytes())
        ))
    }

    pub fn verify(&self, token: &str) -> Result<JwtClaims, AppError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AppError::Unauthorized("malformed token".into()));
        }
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|_| AppError::Unauthorized("bad signature encoding".into()))?;
        let sig = Signature::from_slice(&sig_bytes)
            .map_err(|_| AppError::Unauthorized("bad signature length".into()))?;
        self.verifying_key
            .verify(signing_input.as_bytes(), &sig)
            .map_err(|_| AppError::Unauthorized("signature verification failed".into()))?;

        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| AppError::Unauthorized("bad payload encoding".into()))?;
        let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|_| AppError::Unauthorized("unparseable claims".into()))?;

        let now = unix_now();
        if claims.exp < now {
            return Err(AppError::Unauthorized("token expired".into()));
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jwt() -> Jwt {
        Jwt::from_seed(&[7u8; 32]).expect("32 bytes seed valid")
    }

    fn base_claims() -> JwtClaims {
        JwtClaims {
            sub: "user-1".into(),
            email: Some("a@b.com".into()),
            iat: unix_now(),
            exp: unix_now() + 3600,
            extra: Default::default(),
        }
    }

    #[test]
    fn sign_verify_roundtrip() {
        let jwt = test_jwt();
        let token = jwt.sign(&base_claims()).unwrap();
        let v = jwt.verify(&token).unwrap();
        assert_eq!(v.sub, "user-1");
        assert_eq!(v.email.as_deref(), Some("a@b.com"));
    }

    #[test]
    fn tampered_payload_rejected() {
        let jwt = test_jwt();
        let token = jwt.sign(&base_claims()).unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        let mut bad_payload = parts[1].to_string();
        let mid = bad_payload.len() / 2;
        bad_payload.replace_range(mid..mid + 1, "0");
        let bad = format!("{}.{}.{}", parts[0], bad_payload, parts[2]);
        assert!(matches!(jwt.verify(&bad), Err(AppError::Unauthorized(_))));
    }

    #[test]
    fn tampered_signature_rejected() {
        let jwt = test_jwt();
        let token = jwt.sign(&base_claims()).unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        let bad = format!("{}.{}.{}", parts[0], parts[1], "A".repeat(parts[2].len()));
        assert!(matches!(jwt.verify(&bad), Err(AppError::Unauthorized(_))));
    }

    #[test]
    fn expired_token_rejected() {
        let jwt = test_jwt();
        let mut claims = base_claims();
        claims.exp = unix_now() - 1;
        let token = jwt.sign(&claims).unwrap();
        assert!(
            matches!(jwt.verify(&token), Err(AppError::Unauthorized(e)) if e.contains("expired"))
        );
    }

    #[test]
    fn malformed_token_rejected() {
        let jwt = test_jwt();
        assert!(matches!(
            jwt.verify("not.a.jwt"),
            Err(AppError::Unauthorized(_))
        ));
        assert!(matches!(
            jwt.verify("onlyone"),
            Err(AppError::Unauthorized(_))
        ));
    }

    #[test]
    fn wrong_key_rejected() {
        let a = test_jwt();
        let b = Jwt::from_seed(&[8u8; 32]).unwrap();
        let token = a.sign(&base_claims()).unwrap();
        assert!(matches!(b.verify(&token), Err(AppError::Unauthorized(_))));
    }

    fn test_state() -> AppState {
        use std::collections::HashMap;
        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret-key-123".to_string());
        let config = AppConfig {
            project: "admin-kit".into(),
            environment: "test".into(),
            region: "us-west-2".into(),
            table_prefix: "admin-kit-test-".into(),
            secrets,
            dev_mode: true,
            features: crate::config::FeatureFlags::self_host_defaults(),
        };
        // 直接构造 SdkConfig：不触发 IMDS / 凭证链网络请求，保证离线测试。
        let aws_config = aws_config::SdkConfig::builder()
            .region(aws_config::Region::new("us-west-2"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        AppState {
            config,
            jwt: Jwt::from_seed(&[7u8; 32]).unwrap(),
            aws_config,
        }
    }

    #[tokio::test]
    async fn api_key_auth_valid_and_invalid() {
        use axum::body::Body;
        use axum::extract::Extension;
        use axum::http::Request;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        let state = test_state();
        let app = Router::new()
            .route("/protected", get(|_: ApiKeyAuth| async { "ok" }))
            .layer(Extension(state));

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("x-api-key", "secret-key-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("x-api-key", "wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_key_missing_config_is_internal() {
        use axum::body::Body;
        use axum::extract::Extension;
        use axum::http::Request;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        let mut state = test_state();
        state.config.secrets.clear();
        let app = Router::new()
            .route("/protected", get(|_: ApiKeyAuth| async { "ok" }))
            .layer(Extension(state));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("x-api-key", "anything")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}

/// axum 提取器：从 `X-Authorization: Bearer <jwt>` 自动验证并注入声明。
pub struct JwtAuth(pub JwtClaims);

impl FromRequestParts<()> for JwtAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &()) -> Result<Self, Self::Rejection> {
        let state = Extension::<AppState>::from_request_parts(parts, state)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .0;
        // 优先读 X-Authorization：CloudFront OAC 会占用标准的 Authorization
        // header（换成 SigV4 签名），因此业务 JWT 走 X-Authorization。
        let auth = parts
            .headers
            .get("x-authorization")
            .or_else(|| parts.headers.get(AUTHORIZATION))
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing Authorization header".into()))?;
        let token = auth
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("expected 'Bearer <token>'".into()))?;
        let claims = state.jwt.verify(token)?;
        Ok(JwtAuth(claims))
    }
}

/// axum 提取器：从 `X-API-Key` 头验证，与 SSM 配置的 `api_key` 常量时间比较。
pub struct ApiKeyAuth;

impl FromRequestParts<()> for ApiKeyAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &()) -> Result<Self, Self::Rejection> {
        let state = Extension::<AppState>::from_request_parts(parts, state)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .0;
        let key = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing X-API-Key header".into()))?;
        let expected = state
            .config
            .secret("api_key")
            .map(str::to_string)
            .or_else(|| std::env::var("ADMIN_KIT_DEV_API_KEY").ok())
            .ok_or_else(|| AppError::Internal("api_key not configured".into()))?;
        // 常量时间比较防时序攻击
        if !bool::from(expected.as_bytes().ct_eq(key.as_bytes())) {
            return Err(AppError::Unauthorized("invalid api key".into()));
        }
        Ok(Self)
    }
}

// --- 工具 ---

/// 进程级单调 clock 兜底（测试场景对 iat 时间来源不敏感）。
static FAKE_CLOCK: AtomicU64 = AtomicU64::new(0);

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|_| FAKE_CLOCK.fetch_add(1, Ordering::Relaxed))
}

/// 基于 `rand` 的 32 字节随机数（用于密钥生成）。
fn getrandom_32(buf: &mut [u8]) {
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(buf);
}

// --- 辅助：JWT seed 的来源（SSM 优先，dev 降级到环境变量） ---

/// 从配置解析 Ed25519 seed。生产：SSM `jwt_seed`；开发：`ADMIN_KIT_DEV_JWT_SEED`。
pub fn resolve_jwt_seed(config: &AppConfig) -> anyhow::Result<Vec<u8>> {
    if let Some(b64) = config.secret("jwt_seed") {
        let decoded = URL_SAFE_NO_PAD
            .decode(b64.as_bytes())
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()))
            .map_err(|_| anyhow::anyhow!("jwt_seed in SSM is not valid base64"))?;
        if decoded.len() != 32 {
            anyhow::bail!("jwt_seed must decode to 32 bytes, got {}", decoded.len());
        }
        return Ok(decoded);
    }
    if let Ok(b64) = std::env::var("ADMIN_KIT_DEV_JWT_SEED") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|_| anyhow::anyhow!("ADMIN_KIT_DEV_JWT_SEED is not valid base64"))?;
        if decoded.len() != 32 {
            anyhow::bail!("ADMIN_KIT_DEV_JWT_SEED must decode to 32 bytes");
        }
        return Ok(decoded);
    }
    anyhow::bail!("no JWT seed: set ADMIN_KIT_SECRETS_PATH (SSM jwt_seed) or ADMIN_KIT_DEV_JWT_SEED");
}

#[cfg(test)]
mod seed_tests {
    use super::*;

    #[test]
    fn seed_from_config_secret() {
        use std::collections::HashMap;
        let mut secrets = HashMap::new();
        let seed = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        secrets.insert("jwt_seed".to_string(), seed);
        let config = AppConfig {
            project: "admin-kit".into(),
            environment: "test".into(),
            region: "us-west-2".into(),
            table_prefix: "admin-kit-test-".into(),
            secrets,
            dev_mode: false,
            features: crate::config::FeatureFlags::self_host_defaults(),
        };
        assert_eq!(resolve_jwt_seed(&config).unwrap(), vec![1u8; 32]);
    }
}
