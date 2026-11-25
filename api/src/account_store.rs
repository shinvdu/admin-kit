//! 账号存储端口（端口与适配器分离）。

use async_trait::async_trait;

use crate::account::{Account, AccountError, AccountListOpts, AccountPage, AccountStats, Role};

/// 账号字段更新（`None` 表示不修改）。
#[derive(Debug, Clone, Default)]
pub struct AccountPatch {
    pub role: Option<Role>,
    pub disabled: Option<bool>,
}

#[async_trait]
pub trait AccountStore: Send + Sync {
    /// 适配器类型（health/日志：`memory` / `dynamodb`）。
    fn kind(&self) -> &'static str;

    /// 创建账号：邮箱与账号项必须原子写入，邮箱已存在 → `DuplicateEmail`。
    async fn create(&self, account: Account) -> Result<(), AccountError>;

    /// 按 owner（`user:<uuid>`）强一致读取。
    async fn get(&self, owner: &str) -> Result<Option<Account>, AccountError>;

    /// 按规范化邮箱查找（登录用）。
    async fn find_by_email(&self, normalized_email: &str) -> Result<Option<Account>, AccountError>;

    /// 管理后台分页（GSI 倒序，含 q/role/status 过滤）。
    async fn list(&self, opts: AccountListOpts) -> Result<AccountPage, AccountError>;

    /// 更新角色/禁用状态；任何实际变更都会递增 `session_version`，使旧会话立即失效。
    async fn update(
        &self,
        owner: &str,
        patch: AccountPatch,
        now_ms: u64,
    ) -> Result<Account, AccountError>;

    /// 重置密码哈希；递增 `session_version`。
    async fn change_password(
        &self,
        owner: &str,
        password_hash: &str,
        now_ms: u64,
    ) -> Result<Account, AccountError>;

    /// 统计有效管理员（role=admin 且未禁用）。
    async fn count_active_admins(&self) -> Result<usize, AccountError>;

    /// 用户总数 / 活跃 / 禁用统计。
    async fn stats(&self) -> Result<AccountStats, AccountError>;
}
