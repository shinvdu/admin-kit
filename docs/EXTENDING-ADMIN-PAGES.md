# 扩展 admin-kit 页面：专用 Rust API + 明确 GSI 访问模式

- 适用范围：仓库根（Soybean Admin + Rust Lambda + DynamoDB）
- 编写日期：2026-11-25
- 参考实现：`api/src/admin.rs`（用户管理）、`web/src/views/users/`
- 原则一句话：**一个后台页面 = 一组专用 Rust API = 一组明确的 DynamoDB 访问模式；不要做“通用任意表查询后台”。**

---

## 0. 给执行 Agent 的开工指令（可直接复制）

```text
你正在 admin-kit 仓库中新增后台页面：<FEATURE>。

硬性流程：
1. 先阅读 docs/EXTENDING-ADMIN-PAGES.md。
2. 在动手写代码前，先填写 §2 的 Access Pattern Sheet（页面/查询/Key/索引/排序/过滤/分页/写入），
   把它贴到 PR 描述或交付说明里。没有这张表，不允许写 Rust API。
3. 只实现该页面需要的 API；不要新增通用 filter/sort DSL，不要新增 Scan 权限。
4. 后端按 §4 增加 handler + store；前端按 §6 增加页/API client/路由/i18n。
5. 按 §8 补测试，按 §9 更新 CFN（如新增 GSI），最后跑 §10 的验收命令。
6. 完成后报告：Access Pattern Sheet、API、GSI/成本影响、测试结果、已知限制。
```

---

## 1. 这条规则解决什么问题

管理后台常见的“通用查询”做法是：

```text
GET /api/admin/table?table=orders&filter[status]=paid&filter[user]=42&sort=-created_at&page=3
```

在关系数据库里这是自然能力；在 DynamoDB 里它会迅速变成：

- 任意字段过滤 → 全表 Scan 或海量 GSI；
- 任意列排序 → 每个排序组合一条索引；
- 页码/总数 → 昂贵 Count 或额外索引表；
- JOIN/报表/模糊搜索 → 根本不适合。

因此 admin-kit 的扩展方式固定为：

> **页面驱动 API，API 驱动 Key 设计。**
> 每个页面先写清楚“它要怎样查”，再写 handler；不把 SQL 习惯带进 DynamoDB。

**适合：** 已知运营页面（用户、订单、内容审核、配置列表等）。
**不适合：** BI、临时任意查询、跨实体自由 JOIN。此类需求应使用：

- DynamoDB Export → S3 → Athena（只读报表/导出）；
- DynamoDB Streams → Postgres/Aurora 读模型（复杂查询）；
- OpenSearch（全文/多维搜索）。

---

## 2. 第一步：填 Access Pattern Sheet（强制）

在写任何 Rust 代码前，先为页面填写下表。每一行代表一个**独立的查询或写入路径**。

| # | 页面/动作 | 触发 | Key 条件 | 索引 | 排序 | 允许的过滤（白名单） | 分页 | 写入 |
|---|---|---|---|---|---|---|---|---|
| 1 | 列表 | `GET /api/admin/orders?status=&cursor=` | `gsi1pk = "ORDERS"` | `gsi1` | `gsi1sk` 倒序 | `status` 枚举 | 游标 | — |
| 2 | 详情 | `GET /api/admin/orders/{id}` | `pk = "ORDER#<id>"`, `sk="META"` | 基表 | — | — | — | — |
| 3 | 按用户 | `GET /api/admin/users/{id}/orders` | `pk="USER#<id>"`, `sk begins_with "ORDER#"` | 基表 | SK | — | 游标 | — |
| 4 | 改状态 | `PATCH /api/admin/orders/{id}` | `pk="ORDER#<id>"` | 基表 | — | — | — | 条件更新 |

必须明确：

- 列表的**默认排序**是什么？（通常 `created_at desc`）
- 哪些过滤条件是**索引支持**的？哪些只是“当前页内过滤”？
- 需要总数吗？（如果需要，先讨论成本；默认不提供）
- 需要几种排序？（每一种额外排序通常意味着一条新 GSI）
- 数据量级、读写比、热点分区？
- 是否需要租户/owner 隔离？Key 里必须包含隔离维度。

> 经验阈值：一个实体如果为了后台列表需要 **3–4 条以上 GSI**，或者需求开始出现“任意组合筛选”，
> 就应该停下来讨论“是否改用关系型读模型”，而不是继续加索引。

---

## 3. 数据建模模板

### 3.1 单表 Key 约定（沿用 admin-kit）

```text
实体主记录：
  pk = "<ENTITY>#<id>"
  sk = "META"

子记录 / 事件 / 明细：
  pk = "<ENTITY>#<id>"
  sk = "<CHILD>#<created_ms:013>#<child_id>"

后台列表索引（GSI1）：
  gsi1pk = "ADMIN#<ENTITY>"
  gsi1sk = "<created_ms:013>#<id>"

按 owner/租户：
  gsi2pk = "OWNER#<owner>"
  gsi2sk = "<created_ms:013>#<id>"

稀疏索引（只索引需要出现的子集）：
  仅当 status = "pending" 的项才写 gsi3pk/gsi3sk
```

要求：

- 主键排序键必须**定长/字典序稳定**（时间用 `{ms:013}`，不要用不补零数字字符串）；
- 实体前缀不要复用 `ACCOUNT#` / `EMAIL#` / `USER#`（admin-kit 已有命名空间）；
- 多租户/多用户隔离必须进 Key，不能只靠 handler 过滤；
- GSI 投影用 `INCLUDE`，只投影列表需要的字段；**绝不投影密码/密钥/正文大字段**；
- 列表读取不要用 `Scan`。

### 3.2 索引选择决策树

```text
需要“按主键/父实体查”？
  └─ 是 → 基表 Query（pk + sk / begins_with）

需要“跨分区列出某类实体”？
  └─ 是 → 合成列表 GSI：gsi1pk="ADMIN#<ENTITY>"

需要“按另一个维度筛选/排序”？
  └─ 是 → 一条专用 GSI（一个维度一条，不要试图复用）

需要“多条件组合筛选”？
  ├─ 条件可穷举且低基数 → 把状态拼进 GSI 排序键（如 STATUS#PAID#...）
  ├─ 条件组合很多 → 只做页面内过滤（明确语义：在当前页/有限页内）
  └─ 任意组合 → 不做；改用 Athena / 关系型读模型

需要总数/聚合？
  └─ 维护计数项 / Streams 更新 / 导出 Athena；不要在请求里 Count

需要模糊/全文？
  └─ 前缀用 begins_with；其余用 OpenSearch
```

### 3.3 过滤的真实语义

DynamoDB 的 `FilterExpression` 是**先读后滤**，按读取容量计费，且 `Limit` 在过滤前生效。

因此：

- 主过滤条件应尽量进入 Key Condition（Query），不要依赖 FilterExpression；
- 如果只能页内过滤，接口文档必须写清楚“过滤只作用于已读取页”，UI 也要说明；
- 不要为了一个低频筛选引入全表 Scan；宁可缩小功能或加专用索引。

---

## 4. 后端实现模板

### 4.1 路由与 API 形状

```text
GET    /api/admin/<resources>?limit=&cursor=&<whitelisted filters>
GET    /api/admin/<resources>/{id}
POST   /api/admin/<resources>
PATCH  /api/admin/<resources>/{id}
```

规则：

- **路径和参数只在该资源的 handler 中定义**，组件/前端不得拼 URL；
- `limit` 默认 20，钳制 1–100；
- `cursor` 为不透明字符串，后端编码/解码，前端原样回传；
- 排序固定（由 GSI 决定）；若支持 `sort`，只允许枚举值，每个值对应明确索引；
- 过滤参数只允许枚举/白名单；非法值 400；
- 列表响应：`{ "items": [...], "next_cursor": "..." | null }`；不返回总页数；
- 写操作返回更新后的实体；冲突/权限错误用明确 code。

### 4.2 Handler 骨架

```rust
use axum::extract::{Extension, Path};
use axum::Json;
use serde::Deserialize;

use crate::auth::AdminAuth;
use crate::error::ApiError;
use crate::extract::ApiQuery;
use crate::AppCtx;

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    /// 只允许白名单枚举
    pub status: Option<String>,
}

pub async fn list_orders(
    _admin: AdminAuth,                 // 服务端鉴权，不能只靠前端菜单
    Extension(ctx): Extension<AppCtx>,
    ApiQuery(query): ApiQuery<ListOrdersQuery>,
) -> Result<Json<OrderListResponse>, ApiError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let status = parse_status(query.status.as_deref())?;

    let page = ctx
        .orders
        .list(OrderListOpts {
            limit,
            cursor: query.cursor,
            status,
        })
        .await?;

    Ok(Json(OrderListResponse {
        items: page.items.into_iter().map(OrderView::from).collect(),
        next_cursor: page.next_cursor,
    }))
}
```

要求：

- 使用 `AdminAuth`（或等价的服务端角色提取器）；不要接受 `X-API-Key` 作为后台身份；
- 写操作要审计日志（只记录 id/动作/状态码，不记录正文/密码）；
- handler 不感知 DynamoDB SDK；通过 store trait 访问数据；
- 错误映射集中：`NotFound` → 404、`Conflict` → 409、`SelfAction/LastAdmin` → 400。

### 4.3 Store port（端口与适配器）

```rust
#[async_trait]
pub trait OrderStore: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn get(&self, id: &str) -> Result<Option<Order>, OrderError>;
    async fn list(&self, opts: OrderListOpts) -> Result<OrderPage, OrderError>;
    async fn create(&self, order: Order) -> Result<(), OrderError>;
    async fn update(&self, id: &str, patch: OrderPatch) -> Result<Order, OrderError>;
}
```

- 不要给 store 增加 `query(any_filter, any_sort)` 这种通用方法；
- 内存实现只用于本地/测试；Dynamo 实现负责 Key/Cursor；
- 复杂的条件更新直接写 SDK 原生表达式（`update_returning` + ExpressionAttributeNames），不要引入 ORM。

### 4.4 DynamoDB 列表实现骨架

参考 `api/src/dynamo_account_store.rs`：

```rust
let start_key = match opts.cursor.as_deref() {
    Some(raw) => {
        let c = decode_cursor(raw)?;                 // base64url(JSON)
        Some(key_map_gsi(&c.pk, &c.sk, &c.gsi1pk, &c.gsi1sk))
    }
    None => None,
};

let (page, last_key) = self.db
    .query_gsi_page::<OrderSummary>(
        "gsi1",
        "gsi1pk",
        "gsi1sk",
        "ADMIN#ORDERS",
        None,                                        // 可选 sk 前缀
        start_key,
        limit,
        false,                                       // GSI 只能最终一致
    )
    .await?;

let next_cursor = last_key.as_ref().map(encode_cursor);
```

游标必须包含 `ExclusiveStartKey` 需要的**全部键**：

```text
pk, sk, gsi1pk, gsi1sk
```

要求：

- 游标是 base64url(JSON)，不要让前端解析/拼接；
- 游标为空或格式错误 → 400，不泄露内部键；
- 不要用 `ExclusiveStartKey` 猜偏移量；
- GSI 最终一致；编辑后详情读基表强一致，列表允许短暂延迟（UI 本地更新）；
- 若过滤导致一页凑不满，可循环查询（见用户管理实现），但必须设置最大循环次数/上限，防止长尾。

---

## 5. 写入与一致性

| 需求 | 做法 |
|---|---|
| 创建 + 唯一性约束 | `TransactWriteItems`，条件 `attribute_not_exists(pk)`（参考账号 EMAIL 唯一性） |
| 乐观锁 | 版本号字段 + `ConditionExpression` |
| 状态变更 | 条件更新 + `updated_at`；必要时递增 `session_version`/`version` |
| 子记录 | 同一 `pk` 下 `sk` 排序；或专用 GSI |
| 计数 | 计数项 `pk="COUNTER#..."`，条件更新或 Streams 聚合 |
| 报表 | 导出 S3 → Athena；不在 Lambda 内扫表 |
| 审计 | 追加 `AUDIT#...` 项或 CloudWatch；不可变、只追加 |

**不要**在请求路径里：

- 全表 Scan；
- 用 `Count` 求总数；
- 用多个 Query 拼 JOIN 并做 N+1（如必须，限制 N 并批量化）；
- 为了“可能以后要用”提前建大量 GSI。

---

## 6. 前端实现模板（Soybean Admin）

### 6.1 文件结构

```text
web/src/
├── views/<feature>/
│   ├── index.vue                 # 列表 + 筛选 + 游标分页
│   └── modules/
│       ├── <feature>-form.vue    # 新建/编辑弹窗
│       └── ...                   # 其他动作弹窗
├── service/api/<feature>.ts      # 唯一 API 客户端（路径集中）
├── typings/api/<feature>.d.ts    # 类型
├── locales/langs/zh-cn.ts        # 文案 key
└── locales/langs/en-us.ts
```

- 路由由 `corepack pnpm gen-route` 生成（`src/router/elegant/*` 是生成文件，不手改）；
- 页面路由加 `meta.roles: ['owner', 'admin']`（或按业务角色）；
- 菜单/路由的权限只是展示层，后端 `AdminAuth` 才是边界。

### 6.2 API 客户端模板

```ts
import { request } from '../request';

export async function fetchOrderList(params: Api.Admin.OrderListParams = {}) {
  const query: Record<string, string | number> = { limit: params.limit ?? 20 };
  if (params.cursor) query.cursor = params.cursor;
  if (params.status) query.status = params.status;

  const result = await request<Api.Admin.OrderListResponse>({
    url: '/admin/orders',
    params: query
  });

  if (result.data) result.data = normalizeList(result.data);
  return result;
}
```

要求：

- 不在组件里出现 `/api/...` 字符串；
- epoch ms → ISO 的转换放在 API 客户端或 `utils/format`，组件只消费展示类型；
- 错误统一用 `getApiError(error)`；401 由全局请求层处理；409/400 由页面展示；
- 非 GET 请求的 `x-amz-content-sha256` 已由全局请求层注入，**不要重复实现**；
- Cookie 会话由 `withCredentials: true` 与同源 `/api` 负责。

### 6.3 列表页模板

```vue
<script setup lang="ts">
const items = ref<Api.Admin.Order[]>([]);
const nextCursor = ref<string | null>(null);
const cursorStack = ref<(string | null)[]>([null]);
const pageIndex = ref(0);
const pageSize = ref(20);
const loading = ref(false);

async function load() {
  loading.value = true;
  const { data, error } = await fetchOrderList({
    limit: pageSize.value,
    cursor: cursorStack.value[pageIndex.value] ?? null,
    status: filters.status || undefined
  });
  loading.value = false;
  if (error) return;
  items.value = data?.items ?? [];
  nextCursor.value = data?.next_cursor ?? null;
}
</script>
```

UI 约束：

- 使用“上一页/下一页/加载更多 + 页大小”，**不要页码跳转和总页数**；
- 显示“已加载 N 条”，不伪造总数；
- 筛选变化时清空游标栈并回到第一页；
- 空状态/错误状态/加载状态必须齐全；
- 删除/禁用等危险动作二次确认；
- 当前用户相关操作（禁用自己/降级自己）在 UI 禁用，但后端仍必须拒绝；
- light/dark、窄屏（375px）都要检查；
- 所有文案走 `$t`；若项目要求多语言，补齐对应 locale key。

---

## 7. 新增 GSI / 基础设施变更

### 7.1 CloudFormation 片段

```yaml
GlobalSecondaryIndexes:
  - IndexName: gsi2
    KeySchema:
      - { AttributeName: gsi2pk, KeyType: HASH }
      - { AttributeName: gsi2sk, KeyType: RANGE }
    Projection:
      ProjectionType: INCLUDE
      NonKeyAttributes: [id, owner, status, created_at, updated_at]
```

同时：

- `AttributeDefinitions` 增加 `gsi2pk`/`gsi2sk`；
- IAM 已允许 `${Table.Arn}/index/*` 的 `Query`；**不要加 `dynamodb:Scan`**；
- 新 GSI 创建后等待 `ACTIVE` 且回填完成再上线列表页；
- 更新栈后检查写入成本/存储增长（每条 GSI 都有写入放大）。

### 7.2 上线顺序

```text
1. 部署 CFN（新增 GSI）→ 等待 ACTIVE/回填完成
2. 部署 Rust API（先用只读列表验证）
3. 部署前端页面
4. 跑 smoke/验收
```

不要先上线前端再补索引；否则列表会退化或失败。

### 7.3 已有数据回填

- 新增 GSI 时，DynamoDB 自动回填已有项（PAY_PER_REQUEST 通常几分钟）；
- 若 Key 值依赖新属性，先写迁移脚本/一次性 Lambda 补属性，再查询；
- 迁移脚本必须幂等、可重跑、限速；禁止在请求路径执行迁移。

---

## 8. 测试要求

### 8.1 后端（必须）

```bash
cargo test -p admin-kit-api
cargo build -p admin-kit-api
```

至少覆盖：

- 列表：limit 钳制、游标翻页、非法游标 400、过滤白名单、空结果；
- 详情：不存在 404；
- 写入：校验失败 400、冲突 409、乐观锁/条件失败；
- 权限：非管理员 403、未登录 401、不能操作自己/最后管理员（如适用）；
- 日志：不包含敏感字段（代码评审检查）。

内存 store 用于离线测试；不要让测试访问真实 AWS。

### 8.2 前端（必须）

```bash
cd web
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build
```

手工/自动验收：

- 未登录访问页面 → 登录；登录后回到原页面；
- 筛选/重置/翻页/页大小；
- 创建/编辑/禁用/删除的成功与失败；
- 409/400 错误落到正确字段或提示；
- 窄屏、light/dark；
- 生产构建无 mock/demo/凭据。

### 8.3 线上冒烟

在 `infra/smoke.sh` 增加该资源的步骤：

```text
登录 → 列表 → 创建 → 详情 → 更新 → 过滤/翻页 → 清理测试数据
```

所有非 GET 请求记得带 `x-amz-content-sha256`。

---

## 9. PR / 交付检查清单

- [ ] Access Pattern Sheet 已填写并评审；
- [ ] 没有新增通用 filter/sort DSL；
- [ ] 没有 `Scan`，没有请求内 `Count`；
- [ ] 过滤参数是白名单枚举，非法值 400；
- [ ] 排序固定或枚举化，且与 GSI 一一对应；
- [ ] 游标包含全部 `ExclusiveStartKey` 键，前端原样回传；
- [ ] GSI 投影最小化，不含密码/密钥/大字段；
- [ ] CFN 已加 GSI/属性定义，IAM 未加 Scan；
- [ ] 后端鉴权/审计/错误映射完成；
- [ ] 前端类型/错误/空状态/loading/分页/主题完成；
- [ ] i18n key 对齐；
- [ ] 测试/typecheck/lint/build 通过；
- [ ] smoke.sh 覆盖新页面并清理测试数据；
- [ ] 文档更新（API/README/索引说明）。

---

## 10. 验收命令

```bash
# 后端
cargo test -p admin-kit-api
cargo build -p admin-kit-api

# 前端
cd web
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build

# 线上
./infra/smoke.sh
```

---

## 11. 反模式（直接拒绝）

| 反模式 | 为什么不行 | 正确做法 |
|---|---|---|
| `?filter[any]=x&sort=any&page=N` | 等于在 DynamoDB 上模拟 SQL | 每页专用 API + 明确 Key |
| 列表接口内部 `Scan` | 成本随表增长，延迟不可控 | GSI Query |
| 返回总页数/总条数 | Count 昂贵或需额外索引 | 游标 + “已加载 N 条” |
| 任意列排序 | 每个排序组合一条 GSI | 固定排序；如必须则加专用 GSI |
| 用 FilterExpression 做主要筛选 | 先读后滤，白读容量 | 过滤进 Key/GSI |
| 一个实体加 5 条 GSI 只为一个后台 | 写入放大、维护成本高 | 缩小需求或改读模型 |
| 前端隐藏按钮当权限 | 可直接调 API | 服务端 `AdminAuth` 校验 |
| 把密码/正文放进 GSI 投影 | 扩大暴露面 | INCLUDE 最小字段 |
| 把 `vue-element-admin` 等旧模板直接搬进来 | Vue2/依赖冲突 | 用 Soybean 页面模式重写 |
| 为了“以后可能用”预先做通用管理台 | 需求和索引都不可知 | 页面驱动，按需扩展 |

---

## 12. 参考：当前“用户管理”如何映射

| 页面动作 | API | 访问模式 |
|---|---|---|
| 用户列表 | `GET /api/admin/users` | GSI `gsi1`，`gsi1pk="USERS"`，`gsi1sk` 倒序；q/role/status 页内过滤 |
| 创建用户 | `POST /api/admin/users` | 事务写 `ACCOUNT#` + `EMAIL#`（邮箱唯一） |
| 改角色/禁用 | `PATCH /api/admin/users/{id}` | 基表条件更新 + `session_version+1` |
| 重置密码 | `PATCH /api/admin/users/{id}`（password） | 基表条件更新 + `session_version+1` |
| 统计 | `GET /api/admin/stats` | GSI 读取后计数（小规模）；大规模应改计数项/Streams |

对应文件：

- 后端：`api/src/admin.rs`、`account_store.rs`、`dynamo_account_store.rs`、`account.rs`
- 前端：`web/src/views/users/index.vue`、`src/service/api/users.ts`
- 基础设施：`infra/template.yaml`
- 冒烟：`infra/smoke.sh`

---

## 13. 相关文档

- 后台底座说明：`README.md`
- AI 使用说明：`docs/AI-USAGE.md`
- 模板调研与 DynamoDB 适配分析：`docs/RESEARCH-ADMIN-TEMPLATES.md`
- Soybean Admin：<https://github.com/soybeanjs/soybean-admin>
- DynamoDB Query 分页：<https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Query.Pagination.html>
- DynamoDB GSI：<https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/GSI.html>
- DynamoDB 导出到 S3：<https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DataExport.html>
- DynamoDB Streams：<https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Streams.html>