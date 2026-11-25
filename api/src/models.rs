//! HTTP 请求 / 响应 DTO。

use serde::{Deserialize, Serialize};

use crate::account::{Account, AccountStats, AccountSummary};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserView {
    pub id: String,
    pub email: String,
    pub role: String,
    pub disabled: bool,
}

impl From<&Account> for UserView {
    fn from(a: &Account) -> Self {
        Self {
            id: a.id.clone(),
            email: a.email.clone(),
            role: a.role.clone(),
            disabled: a.disabled,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UserEnvelope {
    pub user: UserView,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct AdminUser {
    pub id: String,
    pub email: String,
    pub role: String,
    pub disabled: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl From<&Account> for AdminUser {
    fn from(a: &Account) -> Self {
        Self {
            id: a.id.clone(),
            email: a.email.clone(),
            role: a.role.clone(),
            disabled: a.disabled,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

impl From<AccountSummary> for AdminUser {
    fn from(a: AccountSummary) -> Self {
        Self {
            id: a.id,
            email: a.email,
            role: a.role,
            disabled: a.disabled,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UserListResponse {
    pub items: Vec<AdminUser>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AdminListQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    /// 邮箱包含匹配（大小写不敏感）。
    pub q: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total_users: u64,
    pub active_users: u64,
    pub disabled_users: u64,
}

impl From<AccountStats> for StatsResponse {
    fn from(s: AccountStats) -> Self {
        Self {
            total_users: s.total_users,
            active_users: s.active_users,
            disabled_users: s.disabled_users,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub features: FeaturesResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturesResponse {
    pub email_auth: bool,
    pub signup: bool,
    pub admin: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{new_account, AccountCrypto, Role};

    #[test]
    fn user_view_excludes_password_hash() {
        let hash = AccountCrypto::test().hash("password123").unwrap();
        let account = new_account("a@b.com", &hash, Role::Admin, 1_000);
        let view = UserView::from(&account);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["email"], "a@b.com");
        assert_eq!(json["role"], "admin");
        assert_eq!(json["disabled"], false);
        assert!(json.get("password_hash").is_none());
    }

    #[test]
    fn admin_user_includes_timestamps() {
        let hash = AccountCrypto::test().hash("password123").unwrap();
        let mut account = new_account("a@b.com", &hash, Role::User, 1_234);
        account.updated_at = 5_678;
        let json = serde_json::to_value(AdminUser::from(&account)).unwrap();
        assert_eq!(json["created_at"], 1_234);
        assert_eq!(json["updated_at"], 5_678);
        assert!(json.get("session_version").is_none());
    }

    #[test]
    fn config_feature_keys_are_camel_case() {
        let json = serde_json::to_value(ConfigResponse {
            features: FeaturesResponse {
                email_auth: true,
                signup: false,
                admin: true,
            },
        })
        .unwrap();
        assert_eq!(json["features"]["emailAuth"], true);
        assert_eq!(json["features"]["signup"], false);
        assert_eq!(json["features"]["admin"], true);
    }
}
