//! 内存账号存储（本地开发 / 离线测试；重启即清空）。

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::account::{
    decode_account_cursor, encode_account_cursor, matches_filters, stats_from, Account,
    AccountError, AccountListOpts, AccountPage, AccountStats,
};
use crate::account_store::{AccountPatch, AccountStore};

#[derive(Default)]
pub struct MemoryAccountStore {
    /// owner -> account。
    accounts: RwLock<HashMap<String, Account>>,
    /// 规范化邮箱 -> owner（唯一性占位）。
    emails: RwLock<HashMap<String, String>>,
}

impl MemoryAccountStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AccountStore for MemoryAccountStore {
    fn kind(&self) -> &'static str {
        "memory"
    }

    async fn create(&self, account: Account) -> Result<(), AccountError> {
        // 锁顺序固定 accounts -> emails，避免交叉死锁。
        let mut accounts = self.accounts.write().await;
        let mut emails = self.emails.write().await;
        if emails.contains_key(&account.email) {
            return Err(AccountError::DuplicateEmail);
        }
        if accounts.contains_key(&account.owner) {
            return Err(AccountError::Aws("account id collision".into()));
        }
        emails.insert(account.email.clone(), account.owner.clone());
        accounts.insert(account.owner.clone(), account);
        Ok(())
    }

    async fn get(&self, owner: &str) -> Result<Option<Account>, AccountError> {
        Ok(self.accounts.read().await.get(owner).cloned())
    }

    async fn find_by_email(&self, normalized_email: &str) -> Result<Option<Account>, AccountError> {
        let owner = {
            let emails = self.emails.read().await;
            emails.get(normalized_email).cloned()
        };
        match owner {
            Some(owner) => Ok(self.accounts.read().await.get(&owner).cloned()),
            None => Ok(None),
        }
    }

    async fn list(&self, opts: AccountListOpts) -> Result<AccountPage, AccountError> {
        let cursor = match opts.cursor.as_deref() {
            Some(raw) => Some(decode_account_cursor(raw)?.gsi1sk),
            None => None,
        };
        let accounts = self.accounts.read().await;
        let mut items: Vec<Account> = accounts
            .values()
            .filter(|a| {
                cursor
                    .as_deref()
                    .map(|c| a.gsi1sk.as_str() < c)
                    .unwrap_or(true)
            })
            .filter(|a| matches_filters(&a.summary(), opts.q.as_deref(), opts.role, opts.status))
            .cloned()
            .collect();
        items.sort_by(|a, b| b.gsi1sk.cmp(&a.gsi1sk));

        let limit = opts.limit.max(1);
        let has_more = items.len() > limit;
        items.truncate(limit);

        let summaries = items.iter().map(Account::summary).collect::<Vec<_>>();
        let next_cursor = if has_more {
            items
                .last()
                .map(|a| encode_account_cursor(&a.pk, &a.sk, &a.gsi1pk, &a.gsi1sk))
        } else {
            None
        };
        Ok(AccountPage {
            items: summaries,
            next_cursor,
        })
    }

    async fn update(
        &self,
        owner: &str,
        patch: AccountPatch,
        now_ms: u64,
    ) -> Result<Account, AccountError> {
        let mut accounts = self.accounts.write().await;
        let account = accounts.get_mut(owner).ok_or(AccountError::NotFound)?;
        let changed = patch
            .role
            .map(|role| role.as_str() != account.role)
            .unwrap_or(false)
            || patch
                .disabled
                .map(|d| d != account.disabled)
                .unwrap_or(false);
        if let Some(role) = patch.role {
            account.role = role.as_str().to_string();
        }
        if let Some(disabled) = patch.disabled {
            account.disabled = disabled;
        }
        if changed {
            account.session_version += 1;
            account.updated_at = now_ms;
        }
        Ok(account.clone())
    }

    async fn change_password(
        &self,
        owner: &str,
        password_hash: &str,
        now_ms: u64,
    ) -> Result<Account, AccountError> {
        let mut accounts = self.accounts.write().await;
        let account = accounts.get_mut(owner).ok_or(AccountError::NotFound)?;
        account.password_hash = password_hash.to_string();
        account.session_version += 1;
        account.updated_at = now_ms;
        Ok(account.clone())
    }

    async fn count_active_admins(&self) -> Result<usize, AccountError> {
        let accounts = self.accounts.read().await;
        Ok(crate::account::count_active_admins(
            accounts.values().map(Account::summary).collect::<Vec<_>>(),
        ))
    }

    async fn stats(&self) -> Result<AccountStats, AccountError> {
        let accounts = self.accounts.read().await;
        Ok(stats_from(
            accounts.values().map(Account::summary).collect::<Vec<_>>(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{new_account, AccountCrypto, AccountStatus, Role};

    fn acct(email: &str, role: Role, ms: u64) -> Account {
        let hash = AccountCrypto::test().hash("password123").unwrap();
        new_account(email, &hash, role, ms)
    }

    #[tokio::test]
    async fn create_get_find_and_duplicate() {
        let store = MemoryAccountStore::new();
        let a = acct("a@b.com", Role::User, 1_000);
        store.create(a.clone()).await.unwrap();
        assert_eq!(store.get(&a.owner).await.unwrap(), Some(a.clone()));
        assert_eq!(
            store.find_by_email("a@b.com").await.unwrap().map(|x| x.id),
            Some(a.id.clone())
        );
        assert!(matches!(
            store.create(acct("a@b.com", Role::User, 2_000)).await,
            Err(AccountError::DuplicateEmail)
        ));
    }

    #[tokio::test]
    async fn list_order_and_pagination() {
        let store = MemoryAccountStore::new();
        let a = acct("a@b.com", Role::User, 1_000);
        let b = acct("b@b.com", Role::User, 2_000);
        let c = acct("c@b.com", Role::Admin, 3_000);
        for account in [&a, &b, &c] {
            store.create(account.clone()).await.unwrap();
        }
        let p1 = store
            .list(AccountListOpts {
                limit: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(p1.items.len(), 2);
        assert_eq!(p1.items[0].id, c.id);
        assert_eq!(p1.items[1].id, b.id);
        assert!(p1.next_cursor.is_some());

        let p2 = store
            .list(AccountListOpts {
                limit: 2,
                cursor: p1.next_cursor,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(p2.items.len(), 1);
        assert_eq!(p2.items[0].id, a.id);
        assert!(p2.next_cursor.is_none());

        let filtered = store
            .list(AccountListOpts {
                limit: 10,
                q: Some("B@".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].email, "b@b.com");

        let admins = store
            .list(AccountListOpts {
                limit: 10,
                role: Some(Role::Admin),
                status: Some(AccountStatus::Active),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(admins.items.len(), 1);
        assert_eq!(admins.items[0].id, c.id);
    }

    #[tokio::test]
    async fn update_and_password_reset_bump_session_version() {
        let store = MemoryAccountStore::new();
        let a = acct("a@b.com", Role::User, 1_000);
        store.create(a.clone()).await.unwrap();

        let unchanged = store
            .update(
                &a.owner,
                AccountPatch {
                    role: Some(Role::User),
                    disabled: None,
                },
                2_000,
            )
            .await
            .unwrap();
        assert_eq!(unchanged.session_version, 1);

        let promoted = store
            .update(
                &a.owner,
                AccountPatch {
                    role: Some(Role::Admin),
                    disabled: Some(false),
                },
                3_000,
            )
            .await
            .unwrap();
        assert_eq!(promoted.role, "admin");
        assert_eq!(promoted.session_version, 2);

        let disabled = store
            .update(
                &a.owner,
                AccountPatch {
                    role: None,
                    disabled: Some(true),
                },
                4_000,
            )
            .await
            .unwrap();
        assert!(disabled.disabled);
        assert_eq!(disabled.session_version, 3);

        let new_hash = AccountCrypto::test().hash("new-password").unwrap();
        let changed = store
            .change_password(&a.owner, &new_hash, 5_000)
            .await
            .unwrap();
        assert_eq!(changed.session_version, 4);
        assert_eq!(store.count_active_admins().await.unwrap(), 0);

        assert!(matches!(
            store
                .update("user:missing", AccountPatch::default(), 1)
                .await,
            Err(AccountError::NotFound)
        ));
    }

    #[tokio::test]
    async fn stats_counts_active_and_disabled() {
        let store = MemoryAccountStore::new();
        let a = acct("a@b.com", Role::Admin, 1_000);
        let b = acct("b@b.com", Role::User, 2_000);
        let c = acct("c@b.com", Role::User, 3_000);
        for account in [&a, &b, &c] {
            store.create(account.clone()).await.unwrap();
        }
        store
            .update(
                &b.owner,
                AccountPatch {
                    role: None,
                    disabled: Some(true),
                },
                4_000,
            )
            .await
            .unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.total_users, 3);
        assert_eq!(stats.active_users, 2);
        assert_eq!(stats.disabled_users, 1);
        assert_eq!(store.count_active_admins().await.unwrap(), 1);
    }
}
