# GitHub Copilot instructions — admin-kit

This is a Soybean Admin + Rust Lambda + DynamoDB + CloudFront admin starter.

## Read first

- `AI-PROMPT.md` (canonical agent prompt)
- `AGENTS.md` (operational rules)
- `docs/EXTENDING-ADMIN-PAGES.md` (mandatory playbook for new pages)
- `docs/ARCHITECTURE.md`

## Non-negotiable rules

- One page = dedicated Rust endpoints + explicit DynamoDB access pattern.
- Do not add generic table/column query endpoints, filter DSLs, or arbitrary sort.
- Do not use DynamoDB `Scan`, request-time `Count`, or offset/page-number pagination.
- Lists use opaque cursors; responses are `{ items, next_cursor }` (no total).
- Filters are whitelisted enums; invalid values return 400.
- GSI projection must be minimal; never project secrets.
- Server-side RBAC is authoritative; hiding UI is not authorization.
- Non-GET requests must carry `x-amz-content-sha256`; the frontend request layer
  already injects it globally.
- No secrets, demo accounts, mock data, or telemetry in production builds.

## Reference files

- Backend: `api/src/admin.rs`, `api/src/account_store.rs`,
  `api/src/dynamo_account_store.rs`, `api/src/auth.rs`
- Frontend: `web/src/views/users/index.vue`, `web/src/service/api/users.ts`,
  `web/src/store/modules/auth/index.ts`
- Infra: `infra/template.yaml`, `infra/deploy.sh`, `infra/smoke.sh`

## Verification

```bash
cargo test --workspace
cargo build -p admin-kit-api
cd web && corepack pnpm typecheck && corepack pnpm lint && corepack pnpm build
```

If a request needs arbitrary query/BI/JOIN/full-text search, stop and propose a
read model (Aurora/Postgres), Athena, or OpenSearch instead of extending DynamoDB
into a SQL database.