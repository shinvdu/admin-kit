# 让 AI 使用 admin-kit

admin-kit 是“AI-first”的后台底座：仓库里同时提供**可直接复制的提示词**、**Agent 规则**
和**新增页面方法**。目标不是让 AI 随便生成一个 CRUD，而是让 AI 按 DynamoDB 的真实约束
产出可上线、可测试、可扩展的页面。

最显眼的入口：

- 根目录 [`AI-PROMPT.md`](../AI-PROMPT.md) —— 可直接复制给任何编码 Agent
- 根目录 [`AGENTS.md`](../AGENTS.md) —— Claude Code / Codex / Cursor 等通用规则
- 本目录 [`EXTENDING-ADMIN-PAGES.md`](EXTENDING-ADMIN-PAGES.md) —— 新增页面的强制流程

---

## 1. 不同工具怎么用

### Claude Code

把 `AI-PROMPT.md` 的 fenced prompt 粘到第一条消息，或直接说：

```text
Read AI-PROMPT.md, AGENTS.md and docs/EXTENDING-ADMIN-PAGES.md, then add the
<FEATURE> admin page. Follow the Access Pattern Sheet and hard rules exactly.
```

Claude Code 会自动读取根目录 `CLAUDE.md`；它已指向上述文件。

### Codex / Cursor / 其它 Agent

这些工具通常识别根目录 `AGENTS.md`。如果没有自动读取，手工把下面三个文件作为上下文：

```text
AI-PROMPT.md
AGENTS.md
docs/EXTENDING-ADMIN-PAGES.md
```

### GitHub Copilot

`.github/copilot-instructions.md` 已包含核心规则与参考文件路径。

---

## 2. 推荐的上下文顺序

不要一上来让 AI 写页面。按这个顺序给上下文：

```text
1. README.md                     # 项目是什么、不是什么
2. AI-PROMPT.md                  # 规则与流程
3. AGENTS.md                     # 仓库地图/命令/红线
4. docs/ARCHITECTURE.md          # 架构、认证、数据模型
5. docs/EXTENDING-ADMIN-PAGES.md # 新增页面方法（Access Pattern Sheet）
6. api/src/admin.rs              # 参考后端实现
7. web/src/views/users/          # 参考前端实现
```

如果上下文窗口有限，至少给 1、2、5、6、7。

---

## 3. 新页面任务模板

把下面的模板填好，再交给 Agent：

```text
TASK: 在 admin-kit 新增 <FEATURE> 后台页面。

角色：<owner/admin/user>
页面动作：<列表/筛选/详情/创建/编辑/禁用/导出>
数据量级：<当前条数、年增长、峰值读写>
隔离维度：<无 / owner / tenant>
排序要求：<固定 created_at desc / 需要第二种排序?>
筛选要求：<精确枚举列表>
分页要求：游标；页大小 10/20/50；不显示总数
导出/报表：<无 / 说明>
验收用例：<3-5 条>
```

Agent 必须先返回 **Access Pattern Sheet**；没有这张表，不允许写 Rust API。

---

## 4. 期望的交付顺序

```text
Phase 1  Access Pattern Sheet + API 契约
Phase 2  Rust store/handler + 测试
Phase 3  Soybean 页面 + API client + i18n
Phase 4  CFN GSI（如需要）+ 上线顺序
Phase 5  测试/构建/线上 smoke
```

每一步都应可独立评审。不要接受“先写页面，索引以后再说”的交付。

---

## 5. 给 AI 的硬约束（摘要）

- 一个页面 = 一组专用 Rust API = 一组明确的 DynamoDB 访问模式；
- 禁止通用 `/query?table=...`、任意 `filter/sort` DSL；
- 禁止 `Scan`、请求内 `Count`、页码/offset 分页；
- 列表响应只有 `{ items, next_cursor }`；
- 过滤/排序参数必须白名单枚举；非法值 400；
- 游标包含全部 `ExclusiveStartKey` 键；
- GSI 投影最小化，不投影密码/密钥/大字段；
- 服务端 `AdminAuth` 才是权限边界；
- 非 GET 请求需要 `x-amz-content-sha256`（全局请求层已处理）；
- 不提交密钥、demo、mock、`web/dist`、`infra/params.json`。

完整规则见 [`AI-PROMPT.md`](../AI-PROMPT.md)。

---

## 6. 典型提示词

### 新增页面

```text
Read AI-PROMPT.md and docs/EXTENDING-ADMIN-PAGES.md.
Add a <FEATURE> page:
- list with cursor pagination and <filters>
- create/edit actions <...>
- roles <...>
Start by filling the Access Pattern Sheet. Then implement Rust API/store,
Soybean page, tests, and CFN changes if a GSI is needed.
Do not add generic query endpoints or Scan.
```

### 审查现有改动

```text
Review this change against docs/EXTENDING-ADMIN-PAGES.md.
Focus on: generic query leakage, Scan/Count/offset pagination, cursor keys,
GSI projection, server-side RBAC, x-amz-content-sha256, test evidence.
Return findings by severity; do not edit files.
```

### 排障

```text
Debug this admin-kit issue: <symptom>.
Read AGENTS.md first. Check:
- CloudFront x-amz-content-sha256 on mutations
- Cookie flags (HttpOnly/Secure/SameSite) and same-origin /api
- GSI eventual consistency
- session_version revocation
Do not "fix" by adding Scan or client-side tokens.
```

---

## 7. 什么时候必须停下来问人

- 需求变成任意查询/BI/JOIN/全文搜索；
- 一个实体要加第 4 条以上 GSI；
- 要改表 Key、租户隔离、会话语义；
- 需要删除/迁移生产数据；
- AWS 账号/区域/域名/成本不明确。

这些情况下，正确回答通常是“加关系型读模型 / Athena / OpenSearch”，而不是继续堆 DynamoDB 索引。

---

## 8. AI 交付验收清单

- [ ] Access Pattern Sheet 完整，且每个过滤/排序都有归属；
- [ ] 没有新增通用查询/Scan/Count/offset；
- [ ] API 有游标、limit 钳制、白名单校验；
- [ ] Rust 测试与 build 通过；
- [ ] 前端 typecheck/lint/build 通过；
- [ ] GSI/CFN/IAM 变更与回填说明齐全；
- [ ] smoke 覆盖新页面并清理测试数据；
- [ ] 没有密钥/demo/mock/源码映射进入生产产物；
- [ ] 最终报告列出已知限制。

如果 Agent 只交了页面和“应该能跑”，没有 Access Pattern Sheet、测试和 smoke 证据，
不要合并。