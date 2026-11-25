# CLAUDE.md — admin-kit

**English** | [简体中文](CLAUDE.zh-CN.md)

Read [`AI-PROMPT.md`](AI-PROMPT.md) and [`AGENTS.md`](AGENTS.md) before editing.

## Project

Soybean Admin (Vue 3) + Rust/axum Lambda + DynamoDB single table + CloudFront/S3.
The complete reference feature is user management + auth. New pages must follow
[`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md).

## Commands

```bash
cargo test --workspace
cargo build -p admin-kit-api
cd web && corepack pnpm typecheck && corepack pnpm lint && corepack pnpm build
```

## Rules

- One page = dedicated Rust API + explicit DynamoDB access pattern.
- No generic query endpoint, filter/sort DSL, Scan, Count, or offset pagination.
- Cursor pagination only; cursor includes all ExclusiveStartKey keys.
- Filters/sorts are whitelisted enums; invalid values → 400.
- Auth/RBAC server-side; UI hiding is not security.
- No secrets/demo/mock/telemetry in production; no source maps.
- Non-GET requests through CloudFront need `x-amz-content-sha256`; it is injected
  globally in `web/src/service/request/index.ts`.
- Do not commit `web/node_modules`, `web/dist`, `infra/params.json`, or credentials.

## Before adding a page

Fill the Access Pattern Sheet in `docs/EXTENDING-ADMIN-PAGES.md` §2 first.
Use `api/src/admin.rs` and `web/src/views/users/` as the reference.

Stop and ask if the requirement needs arbitrary querying/BI/JOIN/search, a fourth
GSI for one entity, auth/session changes, or destructive migrations.
