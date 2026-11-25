//! DynamoDB 薄封装。
//!
//! 「薄包装，不做 ORM」：不做查询 DSL、不做关系映射、不做迁移工具，
//! 只让 80% 的日常读写操作写起来舒服：
//! - 自动 serde 序列化 / 反序列化（`serde_dynamo`）；
//! - 统一的 [`DynamoError`]，handler 层再映射成 HTTP 状态码；
//! - 4 个业务扩展：`unix_ms` / `query_page` / `put_if_new` / `update_returning`。
//!
//! 复杂条件表达式直接写 `aws_sdk_dynamodb` 原生调用；薄封装的边界是
//! “80% 常用操作舒服”，不是“100% 抽象”。

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// DynamoDB 返回的原始 item 类型（serde_dynamo 需要它作为反序列化输入）。
pub type Item = HashMap<String, aws_sdk_dynamodb::types::AttributeValue>;

/// 主键（`pk` + `sk`）映射。分页 cursor 恢复时使用。
pub type Key = HashMap<String, aws_sdk_dynamodb::types::AttributeValue>;

/// 从 `LastEvaluatedKey` 中取出排序键字符串（单表模型下仅需要 sk 即可恢复 cursor）。
pub fn key_sk(key: &Key) -> Option<String> {
    match key.get("sk") {
        Some(aws_sdk_dynamodb::types::AttributeValue::S(s)) => Some(s.clone()),
        _ => None,
    }
}

/// 构造 `{pk, sk}` 主键映射（用于 `ExclusiveStartKey`）。
pub fn key_map(pk: &str, sk: &str) -> Key {
    use aws_sdk_dynamodb::types::AttributeValue;
    let mut key = Key::new();
    key.insert("pk".into(), AttributeValue::S(pk.into()));
    key.insert("sk".into(), AttributeValue::S(sk.into()));
    key
}

/// 构造带 GSI 键的主键映射（GSI 分页 `ExclusiveStartKey` 需要 4 个键）。
pub fn key_map_gsi(pk: &str, sk: &str, gsi1pk: &str, gsi1sk: &str) -> Key {
    use aws_sdk_dynamodb::types::AttributeValue;
    let mut key = key_map(pk, sk);
    key.insert("gsi1pk".into(), AttributeValue::S(gsi1pk.into()));
    key.insert("gsi1sk".into(), AttributeValue::S(gsi1sk.into()));
    key
}

/// 从 `LastEvaluatedKey` 中取字符串型键值。
pub fn key_string(key: &Key, name: &str) -> Option<String> {
    match key.get(name) {
        Some(aws_sdk_dynamodb::types::AttributeValue::S(s)) => Some(s.clone()),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DynamoError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conditional check failed")]
    ConditionalCheckFailed,
    #[error("aws: {0}")]
    Aws(String),
}

/// 统一错误映射：DynamoDB 错误 → HTTP 语义错误。
impl From<DynamoError> for admin_kit_core::AppError {
    fn from(e: DynamoError) -> Self {
        match e {
            DynamoError::NotFound(m) => Self::NotFound(m),
            DynamoError::ConditionalCheckFailed => {
                Self::Conflict("conditional check failed".into())
            }
            DynamoError::Aws(m) => Self::Internal(m),
        }
    }
}

/// 毫秒时间源（业务数据用；JWT 仍用 `admin_kit_core::unix_now()` 秒）。
pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 单表客户端：一个客户端绑定一张表（表名已带环境前缀）。
#[derive(Clone)]
pub struct DynamoClient {
    client: aws_sdk_dynamodb::Client,
    table: String,
}

impl DynamoClient {
    pub fn new(aws_config: &aws_config::SdkConfig, table: impl Into<String>) -> Self {
        Self {
            client: aws_sdk_dynamodb::Client::new(aws_config),
            table: table.into(),
        }
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    /// 单条读取（最终一致性）。
    pub async fn get<T: DeserializeOwned>(
        &self,
        pk: &str,
        sk: &str,
    ) -> Result<Option<T>, DynamoError> {
        self.get_inner(pk, sk, false).await
    }

    /// 单条读取（强一致）：用于“刚写完读不到”的详情场景。
    pub async fn get_consistent<T: DeserializeOwned>(
        &self,
        pk: &str,
        sk: &str,
    ) -> Result<Option<T>, DynamoError> {
        self.get_inner(pk, sk, true).await
    }

    async fn get_inner<T: DeserializeOwned>(
        &self,
        pk: &str,
        sk: &str,
        consistent: bool,
    ) -> Result<Option<T>, DynamoError> {
        let out = self
            .client
            .get_item()
            .table_name(&self.table)
            .key("pk", aws_sdk_dynamodb::types::AttributeValue::S(pk.into()))
            .key("sk", aws_sdk_dynamodb::types::AttributeValue::S(sk.into()))
            .consistent_read(consistent)
            .send()
            .await
            .map_err(|e| DynamoError::Aws(e.to_string()))?;
        match out.item() {
            Some(item) => serde_dynamo::from_item::<Item, T>(item.clone())
                .map(Some)
                .map_err(|e| DynamoError::Aws(e.to_string())),
            None => Ok(None),
        }
    }

    /// 写入（整体覆盖）。
    pub async fn put<T: Serialize>(&self, item: &T) -> Result<(), DynamoError> {
        let item = serde_dynamo::to_item(item).map_err(|e| DynamoError::Aws(e.to_string()))?;
        self.client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| DynamoError::Aws(e.to_string()))?;
        Ok(())
    }

    /// 仅当该主键项不存在时写入（`attribute_not_exists(pk)`）。
    ///
    /// 注意：DynamoDB 的条件表达式针对**完整主键对应的 item**求值，
    /// 因此即便同一 `pk` 下已有其他 `sk`，只要本 `(pk, sk)` 不存在，条件即成立。
    pub async fn put_if_new<T: Serialize>(&self, item: &T) -> Result<(), DynamoError> {
        let item = serde_dynamo::to_item(item).map_err(|e| DynamoError::Aws(e.to_string()))?;
        self.client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(pk)")
            .send()
            .await
            .map_err(|e| {
                let err = e.into_service_error();
                if err.is_conditional_check_failed_exception() {
                    DynamoError::ConditionalCheckFailed
                } else {
                    DynamoError::Aws(err.to_string())
                }
            })?;
        Ok(())
    }

    /// 事务写入多条 `attribute_not_exists(pk)` 条件项（如 `ACCOUNT#` + `EMAIL#`）。
    ///
    /// 任一条已存在 → 事务取消，统一映射为 [`DynamoError::ConditionalCheckFailed`]。
    pub async fn transact_put_if_new<T: Serialize>(&self, items: &[T]) -> Result<(), DynamoError> {
        let items = items
            .iter()
            .map(serde_dynamo::to_item)
            .collect::<Result<Vec<Item>, _>>()
            .map_err(|e| DynamoError::Aws(e.to_string()))?;
        self.transact_put_items_if_new(items).await
    }

    /// 同 [`Self::transact_put_if_new`]，但接受已序列化的异构 item。
    pub async fn transact_put_items_if_new(&self, items: Vec<Item>) -> Result<(), DynamoError> {
        use aws_sdk_dynamodb::types::{Put, TransactWriteItem};

        if items.is_empty() {
            return Ok(());
        }
        let mut writes = Vec::with_capacity(items.len());
        for item in items {
            writes.push(
                TransactWriteItem::builder()
                    .put(
                        Put::builder()
                            .table_name(&self.table)
                            .set_item(Some(item))
                            .condition_expression("attribute_not_exists(pk)")
                            .build()
                            .map_err(|e| DynamoError::Aws(e.to_string()))?,
                    )
                    .build(),
            );
        }
        self.client
            .transact_write_items()
            .set_transact_items(Some(writes))
            .send()
            .await
            .map_err(|e| {
                let err = e.into_service_error();
                if err.is_transaction_canceled_exception() {
                    DynamoError::ConditionalCheckFailed
                } else {
                    DynamoError::Aws(err.to_string())
                }
            })?;
        Ok(())
    }

    pub async fn delete(&self, pk: &str, sk: &str) -> Result<(), DynamoError> {
        self.client
            .delete_item()
            .table_name(&self.table)
            .key("pk", aws_sdk_dynamodb::types::AttributeValue::S(pk.into()))
            .key("sk", aws_sdk_dynamodb::types::AttributeValue::S(sk.into()))
            .send()
            .await
            .map_err(|e| DynamoError::Aws(e.to_string()))?;
        Ok(())
    }

    /// 按主键 `pk` 查询所有同键记录（SK 升序）。
    pub async fn query<T: DeserializeOwned>(&self, pk: &str) -> Result<Vec<T>, DynamoError> {
        self.query_key_condition("pk", pk).await
    }

    /// 按 GSI 查询（key 为 GSI 分区键列，pk 为值）。
    pub async fn query_index<T: DeserializeOwned>(
        &self,
        index: &str,
        pk: &str,
    ) -> Result<Vec<T>, DynamoError> {
        self.query_key_condition_with_index(index, "gsi1pk", pk, false)
            .await
    }

    async fn query_key_condition<T: DeserializeOwned>(
        &self,
        key_col: &str,
        pk: &str,
    ) -> Result<Vec<T>, DynamoError> {
        self.query_key_condition_with_index("", key_col, pk, true)
            .await
    }

    async fn query_key_condition_with_index<T: DeserializeOwned>(
        &self,
        index: &str,
        key_col: &str,
        pk: &str,
        paginate: bool,
    ) -> Result<Vec<T>, DynamoError> {
        let mut req = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression(format!("{key_col} = :pk"))
            .expression_attribute_values(
                ":pk",
                aws_sdk_dynamodb::types::AttributeValue::S(pk.into()),
            );
        if !index.is_empty() {
            req = req.index_name(index);
        }
        let mut items = Vec::new();
        let mut start_key: Option<Key> = None;
        loop {
            let out = req
                .clone()
                .set_exclusive_start_key(start_key.clone())
                .send()
                .await
                .map_err(|e| DynamoError::Aws(e.to_string()))?;
            for item in out.items() {
                let v: T = serde_dynamo::from_item::<Item, T>(item.clone())
                    .map_err(|e| DynamoError::Aws(e.to_string()))?;
                items.push(v);
            }
            start_key = out.last_evaluated_key().cloned();
            if !paginate || start_key.is_none() {
                break;
            }
        }
        Ok(items)
    }

    /// 分页查询（业务核心扩展）：
    /// - `forward=false` 即 `ScanIndexForward=false`（最新在前）；
    /// - `from_sk` / `to_sk` 构成左闭右开 SK 范围；
    /// - `filter_deleted=true` 追加 `attribute_not_exists(deleted_at)`；
    /// - 返回本页条目与 `LastEvaluatedKey`（用于下一页 cursor）。
    ///
    /// Filter 在 `Limit` 之后执行，因此单页条目可能少于 `limit`。
    #[allow(clippy::too_many_arguments)]
    pub async fn query_page<T: DeserializeOwned>(
        &self,
        pk: &str,
        from_sk: Option<&str>,
        to_sk: Option<&str>,
        start_key: Option<Key>,
        limit: usize,
        forward: bool,
        filter_deleted: bool,
    ) -> Result<(Vec<T>, Option<Key>), DynamoError> {
        use aws_sdk_dynamodb::types::AttributeValue;

        let mut key_cond = String::from("pk = :pk");
        let mut values: HashMap<String, AttributeValue> = HashMap::new();
        values.insert(":pk".into(), AttributeValue::S(pk.into()));
        if let Some(from) = from_sk {
            key_cond.push_str(" AND sk >= :from_sk");
            values.insert(":from_sk".into(), AttributeValue::S(from.into()));
        }
        if let Some(to) = to_sk {
            key_cond.push_str(" AND sk < :to_sk");
            values.insert(":to_sk".into(), AttributeValue::S(to.into()));
        }

        let mut names: HashMap<String, String> = HashMap::new();
        let mut filter: Option<String> = None;
        if filter_deleted {
            names.insert("#deleted_at".into(), "deleted_at".into());
            filter = Some("attribute_not_exists(#deleted_at)".into());
        }

        let mut req = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression(key_cond)
            .set_expression_attribute_values(Some(values))
            .scan_index_forward(forward)
            .limit(limit as i32)
            .set_exclusive_start_key(start_key);
        if !names.is_empty() {
            req = req.set_expression_attribute_names(Some(names));
        }
        if let Some(filter) = filter {
            req = req.filter_expression(filter);
        }

        let out = req
            .send()
            .await
            .map_err(|e| DynamoError::Aws(e.to_string()))?;
        let mut items = Vec::with_capacity(out.items().len());
        for item in out.items() {
            let v: T = serde_dynamo::from_item::<Item, T>(item.clone())
                .map_err(|e| DynamoError::Aws(e.to_string()))?;
            items.push(v);
        }
        Ok((items, out.last_evaluated_key().cloned()))
    }

    /// GSI 分页查询（用于管理后台账号列表）。
    ///
    /// - `gsi_pk_col` / `gsi_sk_col` 传入列名（如 `gsi1pk` / `gsi1sk`），
    ///   通过 `#` 占位避免保留字问题；
    /// - 返回本页条目与 `LastEvaluatedKey`（含主键与 GSI 键，可直接
    ///   [`key_map_gsi`] 重建 cursor）。
    #[allow(clippy::too_many_arguments)]
    pub async fn query_gsi_page<T: DeserializeOwned>(
        &self,
        index: &str,
        gsi_pk_col: &str,
        gsi_sk_col: &str,
        gsi_pk: &str,
        from_gsi_sk: Option<&str>,
        start_key: Option<Key>,
        limit: usize,
        forward: bool,
    ) -> Result<(Vec<T>, Option<Key>), DynamoError> {
        use aws_sdk_dynamodb::types::AttributeValue;

        let mut key_cond = String::from("#gpk = :gpk");
        let mut values: HashMap<String, AttributeValue> = HashMap::new();
        values.insert(":gpk".into(), AttributeValue::S(gsi_pk.into()));
        let mut names: HashMap<String, String> = HashMap::new();
        names.insert("#gpk".into(), gsi_pk_col.into());
        if let Some(from) = from_gsi_sk {
            key_cond.push_str(" AND #gsk <= :from_gsk");
            values.insert(":from_gsk".into(), AttributeValue::S(from.into()));
            // DynamoDB 拒绝未在表达式中使用的 ExpressionAttributeNames，按需添加。
            names.insert("#gsk".into(), gsi_sk_col.into());
        }

        let out = self
            .client
            .query()
            .table_name(&self.table)
            .index_name(index)
            .key_condition_expression(key_cond)
            .set_expression_attribute_values(Some(values))
            .set_expression_attribute_names(Some(names))
            .scan_index_forward(forward)
            .limit(limit as i32)
            .set_exclusive_start_key(start_key)
            .send()
            .await
            .map_err(|e| DynamoError::Aws(e.to_string()))?;

        let mut items = Vec::with_capacity(out.items().len());
        for item in out.items() {
            let v: T = serde_dynamo::from_item::<Item, T>(item.clone())
                .map_err(|e| DynamoError::Aws(e.to_string()))?;
            items.push(v);
        }
        Ok((items, out.last_evaluated_key().cloned()))
    }

    /// 条件更新 + `ReturnValues=ALL_NEW`。
    ///
    /// `update_expression` 必须是合法的 DynamoDB 更新表达式（通常以 `SET` 开头）；
    /// 需要转义保留字时通过 `names` 传入 `#name -> real_name` 映射。
    pub async fn update_returning<T: DeserializeOwned>(
        &self,
        pk: &str,
        sk: &str,
        update_expression: &str,
        condition: Option<&str>,
        names: Option<HashMap<String, String>>,
        values: HashMap<String, aws_sdk_dynamodb::types::AttributeValue>,
    ) -> Result<T, DynamoError> {
        let mut req = self
            .client
            .update_item()
            .table_name(&self.table)
            .key("pk", aws_sdk_dynamodb::types::AttributeValue::S(pk.into()))
            .key("sk", aws_sdk_dynamodb::types::AttributeValue::S(sk.into()))
            .update_expression(update_expression)
            .set_expression_attribute_values(Some(values))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllNew);
        if let Some(names) = names {
            req = req.set_expression_attribute_names(Some(names));
        }
        if let Some(condition) = condition {
            req = req.condition_expression(condition);
        }
        let out = req.send().await.map_err(|e| {
            let err = e.into_service_error();
            if err.is_conditional_check_failed_exception() {
                DynamoError::ConditionalCheckFailed
            } else {
                DynamoError::Aws(err.to_string())
            }
        })?;
        let item = out
            .attributes()
            .ok_or_else(|| DynamoError::Aws("update returned no attributes".into()))?;
        serde_dynamo::from_item::<Item, T>(item.clone())
            .map_err(|e| DynamoError::Aws(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use admin_kit_core::AppError;
    use aws_sdk_dynamodb::types::AttributeValue;

    #[test]
    fn dynamo_error_maps_to_app_error() {
        assert!(matches!(
            AppError::from(DynamoError::NotFound("x".into())),
            AppError::NotFound(_)
        ));
        assert!(matches!(
            AppError::from(DynamoError::ConditionalCheckFailed),
            AppError::Conflict(_)
        ));
        assert!(matches!(
            AppError::from(DynamoError::Aws("e".into())),
            AppError::Internal(_)
        ));
    }

    #[test]
    fn unix_ms_is_millis_since_epoch() {
        // 2020-01-01 之后的合理值，且是毫秒量级。
        assert!(unix_ms() > 1_577_836_800_000);
    }

    #[test]
    fn key_map_and_key_sk_roundtrip() {
        let key = key_map("USER#owner", "FP#1762800000000-x");
        assert_eq!(key_sk(&key).as_deref(), Some("FP#1762800000000-x"));
    }

    #[test]
    fn key_map_gsi_includes_all_four_keys() {
        let key = key_map_gsi("ACCOUNT#user:x", "META", "USERS", "0000000000001#user:x");
        assert_eq!(key_string(&key, "pk").as_deref(), Some("ACCOUNT#user:x"));
        assert_eq!(key_string(&key, "gsi1pk").as_deref(), Some("USERS"));
        assert_eq!(
            key_string(&key, "gsi1sk").as_deref(),
            Some("0000000000001#user:x")
        );
    }

    #[test]
    fn key_sk_missing() {
        let mut key = Key::new();
        key.insert("pk".into(), AttributeValue::S("USER#owner".into()));
        assert_eq!(key_sk(&key), None);
    }
}
