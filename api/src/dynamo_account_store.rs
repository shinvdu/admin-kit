//! DynamoDB 账号存储适配器（事务创建 / GSI 分页 / 条件更新）。
//!
//! 表名由 `ADMIN_KIT_TABLE` 指定（默认 `admin-kit-dev-users`），
//! 需要 `gsi1`（`gsi1pk` / `gsi1sk`）与 `dynamodb:TransactWriteItems` 权限。

use std::collections::HashMap;

use async_trait::async_trait;

use admin_kit_dynamo::{key_map_gsi, DynamoClient, DynamoError, Key};

use crate::account::{
    account_pk, cursor_for_summary, decode_account_cursor, email_item, email_pk, matches_filters,
    stats_from, Account, AccountError, AccountListOpts, AccountPage, AccountStats, AccountSummary,
    ACCOUNT_GSI, ACCOUNT_GSI_PK, ACCOUNT_SK, EMAIL_SK,
};
use crate::account_store::{AccountPatch, AccountStore};

pub struct DynamoAccountStore {
    db: DynamoClient,
}

impl DynamoAccountStore {
    pub fn new(aws_config: &aws_config::SdkConfig, table: impl Into<String>) -> Self {
        Self {
            db: DynamoClient::new(aws_config, table),
        }
    }

    pub fn table_name(&self) -> &str {
        self.db.table()
    }
}

fn map_dynamo_error(e: DynamoError) -> AccountError {
    match e {
        DynamoError::NotFound(_) | DynamoError::ConditionalCheckFailed => AccountError::NotFound,
        DynamoError::Aws(m) => AccountError::Aws(m),
    }
}

fn account_names(keys: &[&str]) -> HashMap<String, String> {
    keys.iter()
        .map(|k| (format!("#{k}"), (*k).to_string()))
        .collect()
}

#[async_trait]
impl AccountStore for DynamoAccountStore {
    fn kind(&self) -> &'static str {
        "dynamodb"
    }

    async fn create(&self, account: Account) -> Result<(), AccountError> {
        let email = email_item(&account);
        let items = vec![
            serde_dynamo::to_item(&account).map_err(|e| AccountError::Aws(e.to_string()))?,
            serde_dynamo::to_item(&email).map_err(|e| AccountError::Aws(e.to_string()))?,
        ];
        match self.db.transact_put_items_if_new(items).await {
            Ok(()) => Ok(()),
            Err(DynamoError::ConditionalCheckFailed) => Err(AccountError::DuplicateEmail),
            Err(DynamoError::Aws(m)) => Err(AccountError::Aws(m)),
            Err(DynamoError::NotFound(m)) => Err(AccountError::Aws(m)),
        }
    }

    async fn get(&self, owner: &str) -> Result<Option<Account>, AccountError> {
        self.db
            .get_consistent::<Account>(&account_pk(owner), ACCOUNT_SK)
            .await
            .map_err(map_dynamo_error)
    }

    async fn find_by_email(&self, normalized_email: &str) -> Result<Option<Account>, AccountError> {
        let email = self
            .db
            .get_consistent::<crate::account::EmailItem>(&email_pk(normalized_email), EMAIL_SK)
            .await
            .map_err(map_dynamo_error)?;
        match email {
            Some(item) => {
                let account = self.get(&item.owner).await?;
                if account.is_none() {
                    tracing::warn!(
                        email_item_owner = %item.owner,
                        "email reservation exists without account row; ignoring"
                    );
                }
                Ok(account)
            }
            None => Ok(None),
        }
    }

    async fn list(&self, opts: AccountListOpts) -> Result<AccountPage, AccountError> {
        let mut start_key: Option<Key> = match opts.cursor.as_deref() {
            Some(raw) => {
                let c = decode_account_cursor(raw)?;
                Some(key_map_gsi(&c.pk, &c.sk, &c.gsi1pk, &c.gsi1sk))
            }
            None => None,
        };
        let limit = opts.limit.clamp(1, 100);
        let mut items: Vec<AccountSummary> = Vec::with_capacity(limit);
        let mut next_cursor: Option<String> = None;

        // DynamoDB 的 Limit 在过滤前生效；循环查询直到凑满 limit 或扫描完 GSI。
        loop {
            if items.len() >= limit {
                break;
            }
            let remaining = limit - items.len();
            let (page, last_key) = self
                .db
                .query_gsi_page::<AccountSummary>(
                    ACCOUNT_GSI,
                    "gsi1pk",
                    "gsi1sk",
                    ACCOUNT_GSI_PK,
                    None,
                    start_key.clone(),
                    remaining,
                    false,
                )
                .await
                .map_err(|e| AccountError::Aws(e.to_string()))?;

            let mut index = 0;
            while index < page.len() {
                let item = &page[index];
                index += 1;
                if matches_filters(item, opts.q.as_deref(), opts.role, opts.status) {
                    items.push(item.clone());
                    if items.len() == limit {
                        // 若本页还有未读条目，或用尽了页但 Dynamo 还有下一页，则确有后续。
                        let more_in_page = index < page.len();
                        if more_in_page || last_key.is_some() {
                            next_cursor = items.last().map(cursor_for_summary);
                        }
                        return Ok(AccountPage { items, next_cursor });
                    }
                }
            }

            match last_key {
                Some(k) => start_key = Some(k),
                None => break,
            }
        }

        Ok(AccountPage { items, next_cursor })
    }

    async fn update(
        &self,
        owner: &str,
        patch: AccountPatch,
        now_ms: u64,
    ) -> Result<Account, AccountError> {
        let current = self.get(owner).await?.ok_or(AccountError::NotFound)?;
        let mut sets: Vec<&str> = Vec::new();
        let mut values: HashMap<String, aws_sdk_dynamodb::types::AttributeValue> = HashMap::new();
        let mut names: HashMap<String, String> = HashMap::new();

        if let Some(role) = patch.role {
            if role.as_str() != current.role {
                sets.push("#role = :role");
                values.insert(
                    ":role".into(),
                    aws_sdk_dynamodb::types::AttributeValue::S(role.as_str().into()),
                );
                names.insert("#role".into(), "role".into());
            }
        }
        if let Some(disabled) = patch.disabled {
            if disabled != current.disabled {
                sets.push("#disabled = :disabled");
                values.insert(
                    ":disabled".into(),
                    aws_sdk_dynamodb::types::AttributeValue::Bool(disabled),
                );
                names.insert("#disabled".into(), "disabled".into());
            }
        }
        if sets.is_empty() {
            return Ok(current);
        }
        sets.push("#updated_at = :now");
        sets.push("#session_version = #session_version + :one");
        values.insert(
            ":now".into(),
            aws_sdk_dynamodb::types::AttributeValue::N(now_ms.to_string()),
        );
        values.insert(
            ":one".into(),
            aws_sdk_dynamodb::types::AttributeValue::N("1".into()),
        );
        names.extend(account_names(&["updated_at", "session_version"]));

        let expr = format!("SET {}", sets.join(", "));
        self.db
            .update_returning::<Account>(
                &account_pk(owner),
                ACCOUNT_SK,
                &expr,
                Some("attribute_exists(pk)"),
                Some(names),
                values,
            )
            .await
            .map_err(map_dynamo_error)
    }

    async fn change_password(
        &self,
        owner: &str,
        password_hash: &str,
        now_ms: u64,
    ) -> Result<Account, AccountError> {
        let mut values: HashMap<String, aws_sdk_dynamodb::types::AttributeValue> = HashMap::new();
        values.insert(
            ":hash".into(),
            aws_sdk_dynamodb::types::AttributeValue::S(password_hash.into()),
        );
        values.insert(
            ":now".into(),
            aws_sdk_dynamodb::types::AttributeValue::N(now_ms.to_string()),
        );
        values.insert(
            ":one".into(),
            aws_sdk_dynamodb::types::AttributeValue::N("1".into()),
        );
        let names = account_names(&["password_hash", "updated_at", "session_version"]);
        self.db
            .update_returning::<Account>(
                &account_pk(owner),
                ACCOUNT_SK,
                "SET #password_hash = :hash, #updated_at = :now, #session_version = #session_version + :one",
                Some("attribute_exists(pk)"),
                Some(names),
                values,
            )
            .await
            .map_err(map_dynamo_error)
    }

    async fn count_active_admins(&self) -> Result<usize, AccountError> {
        let items = self
            .db
            .query_index::<AccountSummary>(ACCOUNT_GSI, ACCOUNT_GSI_PK)
            .await
            .map_err(|e| AccountError::Aws(e.to_string()))?;
        Ok(crate::account::count_active_admins(items))
    }

    async fn stats(&self) -> Result<AccountStats, AccountError> {
        // 使用 GSI 分页查询而非 Scan 全表。
        let items = self
            .db
            .query_index::<AccountSummary>(ACCOUNT_GSI, ACCOUNT_GSI_PK)
            .await
            .map_err(|e| AccountError::Aws(e.to_string()))?;
        Ok(stats_from(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountCrypto, Role};

    fn aws_config() -> aws_config::SdkConfig {
        aws_config::SdkConfig::builder()
            .region(aws_config::Region::new("us-west-2"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build()
    }

    #[test]
    fn table_name_comes_from_constructor() {
        let store = DynamoAccountStore::new(&aws_config(), "admin-kit-dev-users");
        assert_eq!(store.table_name(), "admin-kit-dev-users");
    }

    #[test]
    fn expression_names_cover_reserved_fields() {
        let names = account_names(&["role", "disabled", "updated_at", "session_version"]);
        assert_eq!(names.get("#role").map(String::as_str), Some("role"));
        assert_eq!(names.get("#disabled").map(String::as_str), Some("disabled"));
        assert_eq!(
            names.get("#session_version").map(String::as_str),
            Some("session_version")
        );
    }

    #[test]
    fn account_shape_roundtrips_serde_dynamo() {
        let hash = AccountCrypto::test().hash("password123").unwrap();
        let account = crate::account::new_account("a@b.com", &hash, Role::Admin, 1_000);
        let item: admin_kit_dynamo::Item = serde_dynamo::to_item(&account).unwrap();
        let back: Account = serde_dynamo::from_item(item).unwrap();
        assert_eq!(back, account);
    }
}
