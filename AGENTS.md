# AGENTS.md — admin-kit

This file is for AI coding agents (Claude Code, Codex, Cursor, Copilot, etc.).
Read [`AI-PROMPT.md`](AI-PROMPT.md) first; it contains the canonical prompt and
the non-negotiable rules. This file is the short operational reference.

---

## What this repository is

admin-kit is a narrow, production-shaped admin starter:

- **Frontend**: Soybean Admin `v2.3.0` (Vue 3 + Vite + TypeScript + Naive UI + UnoCSS)
- **Backend**: Rust + axum + AWS Lambda Web Adapter (one Lambda)
- **Data**: DynamoDB single table + one GSI for user listing
- **Delivery**: CloudFront + S3 + Lambda Function URL (OAC)
- **Session**: HttpOnly Cookie + `session_version` immediate revocation

The only complete feature is **user management + authentication**. Treat it as
the reference pattern, not as a generic table browser.

---

## Repository map

| Path | Purpose |
|---|---|
| `api/src/main.rs` | router assembly, store selection, Lambda/local bootstrap |
| `api/src/handlers.rs` | login / me / logout / change password |
| `api/src/admin.rs` | user list/create/patch/stats (reference page) |
| `api/src/auth.rs` | `SessionAuth` / `AdminAuth`, per-request account validation |
| `api/src/session.rs` | Cookie set/clear helpers |
| `api/src/dynamo_account_store.rs` | DynamoDB adapter, cursor, conditional updates |
| `api/src/memory_account_store.rs` | offline adapter for tests |
| `crates/core/` | config, Ed25519 JWT, AppError, axum/Lambda bootstrap |
| `crates/dynamo/` | thin DynamoDB wrapper (serde, query pagination, transactions) |
| `web/src/views/users/` | reference Soybean page (list + create + actions) |
| `web/src/service/api/` | API clients (the only place URLs live) |
| `web/src/service/request/index.ts` | axios adapter + `x-amz-content-sha256` |
| `web/src/store/modules/auth/index.ts` | Cookie-session state machine |
| `infra/template.yaml` | CloudFormation: DynamoDB, S3, Lambda, CloudFront, IAM |
| `infra/deploy.sh` | build + deploy + frontend sync + health check |
| `infra/smoke.sh` | online end-to-end smoke test |
| `docs/EXTENDING-ADMIN-PAGES.md` | mandatory playbook for new pages |

---

## Commands

```bash
# backend
cargo test --workspace
cargo build -p admin-kit-api

# frontend
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build

# local backend (offline memory store)
AWS_EC2_METADATA_DISABLED=true AWS_REGION=us-west-2 \
ADMIN_KIT_PROJECT=admin-kit ADMIN_KIT_ENV=dev \
ADMIN_KIT_DEV_JWT_SEED="$(openssl rand -base64 32)" \
ADMIN_KIT_DEV_ADMIN_EMAIL=admin@example.com \
ADMIN_KIT_DEV_ADMIN_PASSWORD=devpassword123 \
ADMIN_KIT_COOKIE_SECURE=false ADMIN_KIT_STORE=memory PORT=8090 \
cargo run -p admin-kit-api

# local frontend (after backend is on :8090)
cd web && corepack pnpm dev

# deploy (requires AWS credentials in .env; creates real resources)
cp infra/params.example.json infra/params.json
./infra/deploy.sh

# online smoke (needs a deployed stack)
./infra/smoke.sh
```

---

## Hard rules

1. **No generic query API.** No `/api/admin/query?table=...`, no arbitrary
   filter/sort DSL, no SQL emulation.
2. **No Scan, no request-time Count, no offset/page-number pagination.**
   Lists use GSI/base Query + opaque cursor.
3. **One page = dedicated endpoints + explicit access patterns.** Fill the
   Access Pattern Sheet before code (`docs/EXTENDING-ADMIN-PAGES.md` §2).
4. **Filters are whitelisted enums.** Invalid values return 400.
   If a filter is not index-backed, document that it is page-local.
5. **Sort is fixed or enum-backed.** Every sort maps to a known SK/GSI SK.
6. **Cursor includes all `ExclusiveStartKey` keys** (pk, sk, gsi1pk, gsi1sk).
7. **GSI projection is minimal.** Never project password hashes or large fields.
8. **Auth is server-side.** Frontend menu visibility is not authorization.
9. **No secrets in the browser, localStorage, logs, or repo.**
   Never log emails, passwords, Cookies, or tokens.
10. **CloudFront writes need `x-amz-content-sha256`.** It is injected globally
    in `web/src/service/request/index.ts`; do not duplicate it in components.
11. **No demo/mock/Apifox data in production builds.** No third-party telemetry.
12. **HTML no-cache, hashed assets long cache.** Hash routing avoids SPA fallback.

---

## Workflow for a new page

```text
1. Fill Access Pattern Sheet (screen/action, key, index, sort, filters, pagination, writes)
2. Design cursor + key layout
3. Rust: store port + memory adapter + Dynamo adapter + AdminAuth handler
4. Frontend: API client + typings + Soybean page + route + i18n
5. Infra: add GSI if needed, wait for backfill, update IAM only for Query
6. Tests: cargo test --workspace; typecheck/lint/build; smoke
7. Report: patterns, files, API, GSI cost, test evidence, limitations
```

Reference implementation:

- backend: `api/src/admin.rs` + `api/src/dynamo_account_store.rs`
- frontend: `web/src/views/users/` + `web/src/service/api/users.ts`
- infra: `infra/template.yaml` (`gsi1`)
- smoke: `infra/smoke.sh`

---

## Stop and ask the human when

- the request needs arbitrary query/BI/JOIN/full-text search;
- a fourth GSI for one entity is about to be added;
- auth/session semantics, table keys, or tenant isolation would change;
- a destructive migration or production data deletion is needed;
- deployment account/domain/region is unclear.

---

## Definition of done

- Access Pattern Sheet completed and reviewed;
- Rust tests + build pass;
- frontend typecheck/lint/build pass;
- no Scan/Count/generic query introduced;
- GSI/infra change documented with backfill/cost impact;
- smoke.sh covers the new page and cleans up test data;
- no secrets/demo/mock in `web/dist`;
- final report includes known limitations.

If a requirement conflicts with these rules, stop and explain the trade-off
instead of silently implementing a SQL-like layer on DynamoDB.