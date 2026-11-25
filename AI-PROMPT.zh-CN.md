# AI 提示词 —— 复制给 Agent

[English](AI-PROMPT.md) | **简体中文**

> 这是 admin-kit 的正式提示词（中文版）。它刻意写得很明确：这个仓库
> 可以被 AI 安全扩展，但不会把 DynamoDB 变成假的 SQL 数据库。

---

```text
你是一个 AI 编码 Agent，正在 admin-kit 仓库工作：
Soybean Admin（Vue 3）+ Rust（axum）Lambda + DynamoDB 单表 + S3/CloudFront。

## 动手前先读

按顺序阅读：

1. README.zh-CN.md
2. AGENTS.zh-CN.md
3. docs/AI-USAGE.md
4. docs/EXTENDING-ADMIN-PAGES.md
5. docs/ARCHITECTURE.md
6. 参考实现：
   - 后端：api/src/admin.rs、api/src/account_store.rs、api/src/dynamo_account_store.rs
   - 前端：web/src/views/users/index.vue、web/src/service/api/users.ts

在你能够说清楚页面的访问模式之前，不要开始写代码。

## 不可协商的设计规则

1. 一个页面 = 一组专用 Rust 接口。
   禁止做通用接口，例如：
     /api/admin/table?table=x&filter[...]=y&sort=any&page=3
   禁止新增通用 filter/sort DSL。

2. 每个列表查询都必须映射到明确的 DynamoDB 访问模式：
   - 基表 Query（pk/sk），或
   - 每个访问模式一条 GSI。
   禁止 Scan；禁止在请求中 COUNT(*)；禁止页码/offset 模拟分页；
   禁止任意列排序。过滤参数是白名单枚举；非法值返回 400。

3. 分页使用不透明游标：
   - 服务端编码/解码，客户端原样回传；
   - 游标必须包含 ExclusiveStartKey 需要的所有键
     （pk、sk，使用索引时还要 gsi 键）；
   - 返回 { items, next_cursor }；不返回总数/总页数。

4. DynamoDB 规则：
   - 单表；先设计 Key，再写 handler；
   - GSI 投影最小化（INCLUDE 只放 UI 字段，绝不投影密钥）；
   - GSI 是最终一致；详情读基表；
   - 唯一性约束用 TransactWriteItems + attribute_not_exists(pk)；
   - 状态变更/乐观锁用条件更新；
   - 新增 GSI 要改 infra/template.yaml，并等待回填；
   - 不要加 dynamodb:Scan 权限。

5. 认证与安全：
   - 服务端角色校验是强制的；前端隐藏菜单不是权限；
   - 复用现有 AdminAuth/Cookie 会话模式；
   - token/密码不进入 localStorage；浏览器不持有 AWS 凭证；
   - 审计只记录 id/动作/状态，不记录邮箱/密码/Cookie 正文；
   - 经过 CloudFront 的非 GET 请求必须带 x-amz-content-sha256；
     它已在 web/src/service/request/index.ts 全局注入，组件里不要重复实现。

6. 前端：
   - 使用 Soybean Admin + Naive UI + UnoCSS，不引入新的 UI/CSS 框架；
   - API 路径只写在 web/src/service/api/*.ts；
   - 游标栈（上一页/下一页/加载更多）+ 页大小选择；不要总页数；
   - loading/空状态/错误状态；明亮/暗色；窄屏检查；
   - 所有用户可见文案走 i18n；
   - 路由权限放在 meta.roles，但后端仍是权威边界。

7. 范围纪律：
   - 只实现这个页面需要的功能；
   - 不要预建“以后可能用”的索引、通用后台或分析能力；
   - 不要引入 Directus/AdminJS/Refine/低代码后端；
   - 如果需求是任意查询/BI/全文搜索，停下来，提出关系型读模型
     （Aurora/Postgres）、DynamoDB Export → S3 → Athena，或 OpenSearch。

## 第一个必交产物：Access Pattern Sheet

写代码前，在 PR/回复中给出这张表：

| # | 页面/动作 | 触发 | Key 条件 | 索引 | 排序 | 白名单过滤 | 分页 | 写入 |
|---|---|---|---|---|---|---|---|---|
| 1 | 列表 | GET /api/admin/... | ... | gsi1 | gsi1sk 倒序 | status 枚举 | 游标 | — |
| 2 | 详情 | GET .../{id} | pk=..., sk=META | 基表 | — | — | — | — |
| 3 | 更新 | PATCH .../{id} | pk=... | 基表 | — | — | — | 条件更新 |

同时说明：
- 预计数据量 / 热点分区；
- 租户/owner 隔离维度（必须进 Key）；
- 每条 GSI 为什么必要；
- 过滤是索引支持还是仅页内过滤（必须写清语义）。

## 实施流程

阶段 1 —— 契约
- 定义实体类型、接口、请求/响应 JSON、错误码；
- 定义游标 payload 与 Key 布局；
- 更新 Access Pattern Sheet。

阶段 2 —— Rust
- 增加 store port（ResourceStore）+ 内存适配器用于测试；
- 增加 DynamoDB 适配器（只用 Query，禁止 Scan）；
- 增加 AdminAuth 保护的 handler 和 api/src/main.rs 路由；
- 增加校验（limit 1..100、枚举白名单、不透明游标）；
- 增加审计日志（不含敏感信息）。

阶段 3 —— 前端
- 增加 web/src/service/api/<resource>.ts 与类型；
- 增加 web/src/views/<resource>/index.vue + modules；
- 生成路由（corepack pnpm gen-route），设置 meta.roles；
- 增加 zh-CN/en-US i18n key（项目要求时补齐其它语言）；
- 实现游标分页、筛选、创建/编辑/禁用、错误映射。

阶段 4 —— 基础设施
- 如需要，在 infra/template.yaml 增加属性/GSI；
- 确保 IAM 有 index/* 的 Query，且仍然没有 Scan；
- 说明写入放大/存储成本和回填时间。

阶段 5 —— 验证
- cargo test --workspace
- cargo build -p admin-kit-api
- cd web && corepack pnpm typecheck && corepack pnpm lint && corepack pnpm build
- 扩展 infra/smoke.sh（登录 → 列表 → 创建 → 更新 → 清理）
- 检查：生产 dist 无 demo/mock/apifox/凭据；无 source map；HTML no-cache。

## 必须停下来问人的情况

- 需求需要任意筛选/排序/JOIN/BI（应选读模型）；
- 一个实体要加第 4 条以上 GSI；
- 需要修改认证/会话语义、表 Key 或数据隔离；
- 涉及破坏性迁移或删除生产数据；
- 部署目标/域名/AWS 账号不明确。

## 完成定义

最终报告必须包含：
- Access Pattern Sheet；
- 改动文件；
- API 契约与错误码；
- GSI/基础设施变更及成本/回填说明；
- 测试/typecheck/build 结果；
- smoke/验证结果与清理；
- 已知限制和后续事项。

没有实际运行过的测试，不要声称通过。不要提交密钥、构建产物、
web/node_modules、web/dist 或 infra/params.json。
```

---

## 最小任务包装

有具体页面时，在前面加上：

```text
任务：在 admin-kit 新增 <FEATURE> 后台页面。

使用角色：<roles>
主要动作：<列表/筛选/创建/编辑/禁用/导出?>
预计数据量：<行数、增长>
租户/owner 隔离：<无 | owner id | tenant id>
排序/筛选要求：<精确列表>
导出/报表要求：<无 | 说明>
验收示例：<3-5 个用例>
```

Agent 必须先填 Access Pattern Sheet，再动代码。

---

## 只做审查的 Agent 提示词

```text
请按 docs/EXTENDING-ADMIN-PAGES.md 审查这个 admin-kit 改动。

重点检查：
- 没有通用查询接口/filter DSL；
- 没有 Scan/Count/offset 分页；
- 每个排序/过滤都映射到索引，或明确说明是页内过滤；
- 游标包含所有 ExclusiveStartKey 键；
- GSI 投影不含密钥和大字段；
- 服务端 RBAC；没有前端-only 授权；
- 非 GET 的 x-amz-content-sha256 处理；
- 测试/构建/smoke 证据。

按严重程度报告；不要改文件，返回检查清单。
```