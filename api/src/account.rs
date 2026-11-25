//! 邮箱账号模型与密码哈希（纯逻辑，不感知 AWS）。
//!
//! 单表布局：
//! - `ACCOUNT#{owner}` / `META`：账号主记录（含 argon2 PHC 哈希）；
//! - `EMAIL#{normalized_email}` / `META`：邮箱唯一性与登录查找；
//! - `gsi1pk = "USERS"`、`gsi1sk = "{created_ms:013}#{owner}"`：管理后台分页。
//!
//! owner 命名空间固定为 `user:<uuid>`（与 JWT `sub` 一致）。

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordVerifier, Version};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use admin_kit_core::JwtClaims;

use crate::error::ApiError;
use crate::session::SESSION_TTL_SECS;

pub const ACCOUNT_SK: &str = "META";
pub const EMAIL_SK: &str = "META";
pub const ACCOUNT_GSI: &str = "gsi1";
pub const ACCOUNT_GSI_PK: &str = "USERS";

/// 账号角色。`owner` 仅用于列表过滤 / 展示兼容，创建与更新接口不接受它。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Owner,
    Admin,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "user" => Some(Role::User),
            _ => None,
        }
    }

    pub fn can_admin(self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
}

/// 账号角色字符串是否具备后台权限。`owner` 仅作展示兼容。
pub fn is_admin_role(role: &str) -> bool {
    matches!(role, "admin" | "owner")
}

/// 列表状态过滤。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Disabled,
}

impl AccountStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "active" => Some(AccountStatus::Active),
            "disabled" => Some(AccountStatus::Disabled),
            _ => None,
        }
    }

    fn matches(self, disabled: bool) -> bool {
        match self {
            AccountStatus::Active => !disabled,
            AccountStatus::Disabled => disabled,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub pk: String,
    pub sk: String,
    pub gsi1pk: String,
    pub gsi1sk: String,
    pub id: String,
    pub owner: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub disabled: bool,
    pub session_version: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

/// GSI / 列表投影（不含 `password_hash`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSummary {
    pub id: String,
    pub owner: String,
    pub email: String,
    pub role: String,
    pub disabled: bool,
    pub session_version: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Account {
    pub fn role(&self) -> Role {
        Role::parse(&self.role).unwrap_or(Role::User)
    }

    pub fn summary(&self) -> AccountSummary {
        AccountSummary {
            id: self.id.clone(),
            owner: self.owner.clone(),
            email: self.email.clone(),
            role: self.role.clone(),
            disabled: self.disabled,
            session_version: self.session_version,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// 邮箱唯一性占位项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailItem {
    pub pk: String,
    pub sk: String,
    pub owner: String,
    pub created_at: u64,
}

pub fn account_pk(owner: &str) -> String {
    format!("ACCOUNT#{owner}")
}

pub fn email_pk(normalized_email: &str) -> String {
    format!("EMAIL#{normalized_email}")
}

pub fn account_gsi_sk(created_at: u64, owner: &str) -> String {
    format!("{created_at:013}#{owner}")
}

/// `user:<uuid>`；账号记录里的 `owner` 与 JWT `sub` 一致。
pub fn account_owner(id: &str) -> String {
    format!("user:{id}")
}

/// 从对外 id 还原 owner（校验 uuid 格式，避免伪造路径进入主键）。
pub fn owner_from_account_id(id: &str) -> Result<String, ApiError> {
    let parsed =
        uuid::Uuid::parse_str(id).map_err(|_| ApiError::bad_request("invalid account id"))?;
    Ok(account_owner(&parsed.simple().to_string()))
}

pub fn new_account(email: &str, password_hash: &str, role: Role, now_ms: u64) -> Account {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let owner = account_owner(&id);
    Account {
        pk: account_pk(&owner),
        sk: ACCOUNT_SK.into(),
        gsi1pk: ACCOUNT_GSI_PK.into(),
        gsi1sk: account_gsi_sk(now_ms, &owner),
        id,
        owner,
        email: email.to_string(),
        password_hash: password_hash.to_string(),
        role: role.as_str().into(),
        disabled: false,
        session_version: 1,
        created_at: now_ms,
        updated_at: now_ms,
    }
}

pub fn email_item(account: &Account) -> EmailItem {
    EmailItem {
        pk: email_pk(&account.email),
        sk: EMAIL_SK.into(),
        owner: account.owner.clone(),
        created_at: account.created_at,
    }
}

/// 账号会话 claims：`sub=user:<id>`、`email`、`extra={role, sv}`。
pub fn account_claims(account: &Account) -> JwtClaims {
    let now = admin_kit_core::unix_now();
    let mut extra = serde_json::Map::new();
    extra.insert("role".into(), serde_json::json!(account.role));
    extra.insert("sv".into(), serde_json::json!(account.session_version));
    JwtClaims {
        sub: account.owner.clone(),
        email: Some(account.email.clone()),
        iat: now,
        exp: now + SESSION_TTL_SECS,
        extra,
    }
}

/// 邮箱规范化：trim + 小写；拒绝空值、缺 `@`、多个 `@`、空白、过长。
pub fn normalize_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim().to_lowercase();
    let invalid = || ApiError::bad_request("invalid email");
    if email.is_empty() || email.len() > 254 {
        return Err(invalid());
    }
    if email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid());
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() || local.is_empty() || domain.is_empty() {
        return Err(invalid());
    }
    if local.len() > 64 || !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.')
    {
        return Err(invalid());
    }
    Ok(email)
}

/// 密码策略：8–128 字符，无 NUL。
pub fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < 8 || password.chars().count() > 128 {
        return Err(ApiError::bad_request("password must be 8-128 characters"));
    }
    if password.contains('\0') {
        return Err(ApiError::bad_request(
            "password contains invalid characters",
        ));
    }
    Ok(())
}

/// Argon2id 参数。生产默认 OWASP；测试用低成本参数（`test()`）。
#[derive(Debug, Clone, Copy)]
pub struct AccountCrypto {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for AccountCrypto {
    fn default() -> Self {
        Self {
            m_cost: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

impl AccountCrypto {
    /// 测试专用低成本参数，避免单元测试变慢。
    pub fn test() -> Self {
        Self {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    fn argon2(&self) -> Result<Argon2<'static>, ApiError> {
        let params = Params::new(self.m_cost, self.t_cost, self.p_cost, None)
            .map_err(|e| ApiError::internal(e.to_string()))?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }

    pub fn hash(&self, password: &str) -> Result<String, ApiError> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon2()?
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| ApiError::internal(e.to_string()))
    }

    pub fn verify(&self, stored_hash: &str, password: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return false;
        };
        self.argon2()
            .and_then(|a| {
                a.verify_password(password.as_bytes(), &parsed)
                    .map_err(|e| ApiError::internal(e.to_string()))
            })
            .is_ok()
    }
}

/// 管理后台游标：GSI `LastEvaluatedKey` 需要 4 个键。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCursor {
    pub pk: String,
    pub sk: String,
    pub gsi1pk: String,
    pub gsi1sk: String,
}

pub fn encode_account_cursor(pk: &str, sk: &str, gsi1pk: &str, gsi1sk: &str) -> String {
    let payload = serde_json::to_vec(&AccountCursor {
        pk: pk.into(),
        sk: sk.into(),
        gsi1pk: gsi1pk.into(),
        gsi1sk: gsi1sk.into(),
    })
    .expect("cursor serializes");
    URL_SAFE_NO_PAD.encode(payload)
}

pub fn decode_account_cursor(cursor: &str) -> Result<AccountCursor, AccountError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_bytes())
        .map_err(|_| AccountError::InvalidCursor)?;
    let payload: AccountCursor =
        serde_json::from_slice(&bytes).map_err(|_| AccountError::InvalidCursor)?;
    if payload.pk.is_empty()
        || payload.sk.is_empty()
        || payload.gsi1pk.is_empty()
        || payload.gsi1sk.is_empty()
    {
        return Err(AccountError::InvalidCursor);
    }
    Ok(payload)
}

/// 由列表投影重建游标（账号行的 sk/gsi 键是确定的）。
pub fn cursor_for_summary(summary: &AccountSummary) -> String {
    encode_account_cursor(
        &account_pk(&summary.owner),
        ACCOUNT_SK,
        ACCOUNT_GSI_PK,
        &account_gsi_sk(summary.created_at, &summary.owner),
    )
}

/// 账号存储错误（与 HTTP 层解耦）。
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("account not found")]
    NotFound,
    #[error("email already registered")]
    DuplicateEmail,
    #[error("invalid cursor")]
    InvalidCursor,
    #[error("account store error: {0}")]
    Aws(String),
}

impl From<AccountError> for ApiError {
    fn from(e: AccountError) -> Self {
        match e {
            AccountError::NotFound => ApiError::not_found("account not found"),
            AccountError::DuplicateEmail => ApiError::email_taken(),
            AccountError::InvalidCursor => ApiError::bad_request("invalid cursor"),
            AccountError::Aws(m) => ApiError::internal(m),
        }
    }
}

/// 管理后台列表分页 / 过滤。
#[derive(Debug, Clone, Default)]
pub struct AccountListOpts {
    pub limit: usize,
    pub cursor: Option<String>,
    /// 邮箱包含匹配（大小写不敏感）。
    pub q: Option<String>,
    pub role: Option<Role>,
    pub status: Option<AccountStatus>,
}

#[derive(Debug, Clone, Default)]
pub struct AccountPage {
    pub items: Vec<AccountSummary>,
    pub next_cursor: Option<String>,
}

/// 列表过滤（q / role / status）。
pub fn matches_filters(
    summary: &AccountSummary,
    q: Option<&str>,
    role: Option<Role>,
    status: Option<AccountStatus>,
) -> bool {
    if let Some(q) = q.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty()) {
        if !summary.email.contains(&q) {
            return false;
        }
    }
    if let Some(role) = role {
        if summary.role != role.as_str() {
            return false;
        }
    }
    if let Some(status) = status {
        if !status.matches(summary.disabled) {
            return false;
        }
    }
    true
}

/// 统计有效管理员（role=admin 且未禁用）。
pub fn count_active_admins(accounts: impl IntoIterator<Item = AccountSummary>) -> usize {
    accounts
        .into_iter()
        .filter(|a| a.role == "admin" && !a.disabled)
        .count()
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct AccountStats {
    pub total_users: u64,
    pub active_users: u64,
    pub disabled_users: u64,
}

pub fn stats_from(accounts: impl IntoIterator<Item = AccountSummary>) -> AccountStats {
    let mut stats = AccountStats::default();
    for a in accounts {
        stats.total_users += 1;
        if a.disabled {
            stats.disabled_users += 1;
        } else {
            stats.active_users += 1;
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization() {
        assert_eq!(
            normalize_email("  Alice@Example.COM ").unwrap(),
            "alice@example.com"
        );
        for bad in [
            "",
            "no-at",
            "@example.com",
            "a@",
            "a@@b.com",
            "a b@c.com",
            "a@localhost",
            "a@.com",
            "a@com.",
        ] {
            assert!(normalize_email(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn password_policy() {
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("1234567").is_err());
        assert!(validate_password(&"x".repeat(128)).is_ok());
        assert!(validate_password(&"x".repeat(129)).is_err());
        assert!(validate_password("1234\0 5678").is_err());
    }

    #[test]
    fn account_item_shape_and_claims() {
        let crypto = AccountCrypto::test();
        let hash = crypto.hash("password123").unwrap();
        let account = new_account("a@b.com", &hash, Role::Admin, 1_762_800_000_000);
        assert_eq!(account.pk, format!("ACCOUNT#user:{}", account.id));
        assert_eq!(account.owner, format!("user:{}", account.id));
        assert_eq!(account.gsi1pk, "USERS");
        assert!(account.gsi1sk.starts_with("1762800000000#user:"));
        assert_eq!(account.role, "admin");
        assert_eq!(account.session_version, 1);
        assert!(!account.disabled);

        let item = email_item(&account);
        assert_eq!(item.pk, "EMAIL#a@b.com");
        assert_eq!(item.owner, account.owner);
        assert_eq!(item.sk, "META");

        let claims = account_claims(&account);
        assert_eq!(claims.sub, account.owner);
        assert_eq!(claims.email.as_deref(), Some("a@b.com"));
        assert_eq!(claims.extra.get("role").unwrap(), "admin");
        assert_eq!(claims.extra.get("sv").unwrap(), 1);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn password_hash_roundtrip() {
        let crypto = AccountCrypto::test();
        let hash = crypto.hash("correct horse").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(crypto.verify(&hash, "correct horse"));
        assert!(!crypto.verify(&hash, "wrong"));
        assert!(!crypto.verify("not-a-phc", "x"));
    }

    #[test]
    fn account_owner_parsing() {
        let id = "0192f0c4e6b84a8f9c6e1d2b3a4c5d6e";
        assert_eq!(owner_from_account_id(id).unwrap(), format!("user:{id}"));
        assert!(owner_from_account_id("not-a-uuid").is_err());
    }

    #[test]
    fn cursor_roundtrip_and_invalid() {
        let c = encode_account_cursor("ACCOUNT#user:x", "META", "USERS", "0000000000001#user:x");
        let decoded = decode_account_cursor(&c).unwrap();
        assert_eq!(decoded.pk, "ACCOUNT#user:x");
        assert_eq!(decoded.gsi1sk, "0000000000001#user:x");
        assert!(matches!(
            decode_account_cursor("not-base64!!"),
            Err(AccountError::InvalidCursor)
        ));
        let empty = URL_SAFE_NO_PAD.encode(b"{}");
        assert!(matches!(
            decode_account_cursor(&empty),
            Err(AccountError::InvalidCursor)
        ));
    }

    #[test]
    fn cursor_from_summary_reconstructs_keys() {
        let a = AccountSummary {
            id: "abc".into(),
            owner: "user:abc".into(),
            email: "a@b.com".into(),
            role: "user".into(),
            disabled: false,
            session_version: 1,
            created_at: 1_000,
            updated_at: 1_000,
        };
        let c = decode_account_cursor(&cursor_for_summary(&a)).unwrap();
        assert_eq!(c.pk, "ACCOUNT#user:abc");
        assert_eq!(c.gsi1sk, "0000000001000#user:abc");
    }

    #[test]
    fn role_and_status_parsing() {
        assert_eq!(Role::parse("admin"), Some(Role::Admin));
        assert_eq!(Role::parse("user"), Some(Role::User));
        assert_eq!(Role::parse("owner"), Some(Role::Owner));
        assert!(Role::Admin.can_admin());
        assert!(Role::Owner.can_admin());
        assert!(!Role::User.can_admin());
        assert!(is_admin_role("admin"));
        assert!(is_admin_role("owner"));
        assert!(!is_admin_role("user"));

        assert_eq!(AccountStatus::parse("active"), Some(AccountStatus::Active));
        assert_eq!(
            AccountStatus::parse("disabled"),
            Some(AccountStatus::Disabled)
        );
        assert_eq!(AccountStatus::parse("foo"), None);
    }

    #[test]
    fn filters_and_stats() {
        let mk = |role: &str, disabled: bool, email: &str| AccountSummary {
            id: email.into(),
            owner: format!("user:{email}"),
            email: email.into(),
            role: role.into(),
            disabled,
            session_version: 1,
            created_at: 1,
            updated_at: 1,
        };
        let boss = mk("admin", false, "boss@x.com");
        let old = mk("admin", true, "old@x.com");
        let user = mk("user", false, "u@x.com");

        assert!(matches_filters(&boss, Some("BOSS"), None, None));
        assert!(!matches_filters(&user, Some("boss"), None, None));
        assert!(matches_filters(&boss, None, Some(Role::Admin), None));
        assert!(!matches_filters(&user, None, Some(Role::Admin), None));
        assert!(matches_filters(
            &old,
            None,
            None,
            Some(AccountStatus::Disabled)
        ));
        assert!(!matches_filters(
            &user,
            None,
            None,
            Some(AccountStatus::Disabled)
        ));

        let all = vec![boss, old, user];
        assert_eq!(count_active_admins(all.clone()), 1);
        let stats = stats_from(all);
        assert_eq!(stats.total_users, 3);
        assert_eq!(stats.active_users, 2);
        assert_eq!(stats.disabled_users, 1);
    }
}
