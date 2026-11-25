# AI Prompt — copy this into your agent

**English** | [简体中文](AI-PROMPT.zh-CN.md)

> This file is the canonical prompt for AI coding agents using admin-kit.
> It is intentionally explicit: the repository is designed to be safe for agents
> to extend without turning DynamoDB into a fake SQL database.

---

```text
You are an AI coding agent working in the admin-kit repository:
Soybean Admin (Vue 3) + Rust (axum) Lambda + DynamoDB single table + S3/CloudFront.

## Read before doing anything

Read these files in order:

1. README.md
2. AGENTS.md
3. docs/AI-USAGE.md
4. docs/EXTENDING-ADMIN-PAGES.md
5. docs/ARCHITECTURE.md
6. Reference implementation:
   - backend: api/src/admin.rs, api/src/account_store.rs, api/src/dynamo_account_store.rs
   - frontend: web/src/views/users/index.vue, web/src/service/api/users.ts

Do not start coding until you can state the page's access patterns.

## Non-negotiable design rules

1. One page = one set of dedicated Rust endpoints.
   Never build a generic endpoint such as:
     /api/admin/table?table=x&filter[...]=y&sort=any&page=3
   Never add a generic filter/sort DSL.

2. Every list query must map to an explicit DynamoDB access pattern:
   - base-table Query by pk/sk, or
   - one GSI per listed access pattern.
   No Scan. No request-time COUNT(*). No page numbers/offset emulation.
   No arbitrary column sorting. Filters are a whitelist; invalid values -> 400.

3. Pagination is an opaque cursor:
   - server encodes/decodes the cursor; client passes it back unchanged;
   - cursor includes every key needed for ExclusiveStartKey
     (pk, sk, and gsi keys when using an index);
   - response is { items, next_cursor }; no total count/page count.

4. DynamoDB rules:
   - single table; keys are designed before handlers;
   - GSI projections are minimal (INCLUDE only UI fields; never secrets);
   - GSI reads are eventually consistent; detail reads use the base table;
   - unique constraints use TransactWriteItems with attribute_not_exists(pk);
   - conditional updates for optimistic locking/state transitions;
   - GSI additions happen in infra/template.yaml and require backfill time;
   - do not add dynamodb:Scan permission.

5. Auth/security:
   - server-side role checks are mandatory; frontend menu hiding is not security;
   - use the existing AdminAuth/Cookie-session pattern;
   - no tokens/passwords in localStorage; no credentials in the browser;
   - audit writes: id, action, status only; never log email/password/Cookie bodies;
   - every non-GET request through CloudFront must carry x-amz-content-sha256;
     this is already injected globally in web/src/service/request/index.ts —
     do not re-implement it in components.

6. Frontend:
   - use Soybean Admin + Naive UI + UnoCSS; no new UI/CSS framework;
   - API paths live only in web/src/service/api/*.ts;
   - cursor stack (prev/next/load-more), page-size selector; no total pages;
   - loading/empty/error states; light/dark; narrow-screen check;
   - all user-facing text goes through i18n keys;
   - route permissions belong in meta.roles, but the backend remains authoritative.

7. Scope discipline:
   - implement only what the page needs;
   - do not pre-build "maybe later" indexes, generic admin tools, or analytics;
   - do not introduce Directus/AdminJS/Refine/low-code backends;
   - if the requirement is arbitrary query/BI/full-text search, STOP and propose
     a read model (Aurora/Postgres), DynamoDB Export -> S3 -> Athena, or OpenSearch.

## Required first artifact: Access Pattern Sheet

Before code, produce this table in the PR/response:

| # | Screen/action | Trigger | Key condition | Index | Sort | Whitelisted filters | Pagination | Writes |
|---|---|---|---|---|---|---|---|---|
| 1 | list | GET /api/admin/... | ... | gsi1 | gsi1sk desc | status enum | cursor | — |
| 2 | detail | GET .../{id} | pk=..., sk=META | base | — | — | — | — |
| 3 | update | PATCH .../{id} | pk=... | base | — | — | — | condition update |

Also state:
- expected data volume / hot partitions;
- tenant/owner isolation dimension (must be in the key);
- why each GSI is necessary;
- whether a filter is index-backed or only page-local (document the semantics).

## Implementation workflow

Phase 1 — contract
- define entity types, endpoints, request/response JSON, error codes;
- define cursor payload and key layout;
- update the Access Pattern Sheet.

Phase 2 — Rust
- add a store port (ResourceStore) + memory adapter for tests;
- add a DynamoDB adapter using Query (never Scan);
- add AdminAuth-protected handlers and route registration in api/src/main.rs;
- add validation (limit clamp 1..100, enum whitelists, opaque cursor);
- add audit logging (no secrets).

Phase 3 — frontend
- add web/src/service/api/<resource>.ts and typings;
- add web/src/views/<resource>/index.vue + modules;
- generate route (corepack pnpm gen-route), set meta.roles;
- add zh-CN/en-US i18n keys (and other locales if the project requires them);
- implement cursor pagination, filters, create/edit/disable actions, error mapping.

Phase 4 — infra
- add attributes/GSI to infra/template.yaml if needed;
- ensure IAM has Query on index/* and still no Scan;
- document write amplification/storage impact and backfill timing.

Phase 5 — verify
- cargo test --workspace
- cargo build -p admin-kit-api
- cd web && corepack pnpm typecheck && corepack pnpm lint && corepack pnpm build
- extend infra/smoke.sh for the new page (login -> list -> create -> update -> cleanup)
- verify: no demo/mock/apifox/credentials in dist; no source maps; HTML no-cache.

## Stop and ask the human when

- the requirement needs arbitrary filtering/sorting/JOINs/BI (choose a read model);
- a new GSI would be the 4th+ index for one entity;
- you need to change auth/session semantics, table keys, or data isolation;
- a destructive migration or production data deletion is involved;
- the deployment target/domain/AWS account is unclear.

## Definition of done

Return a report with:
- Access Pattern Sheet;
- files changed;
- API contract and error codes;
- GSI/infra changes and cost/backfill notes;
- test/typecheck/build results;
- smoke/verification results and cleanup;
- known limitations and follow-ups.

Never claim tests passed unless you ran them. Never commit secrets, generated
build output, `web/node_modules`, `web/dist`, or `infra/params.json`.
```

---

## Minimal task wrapper

When you have a concrete page, prepend:

```text
TASK: Add the <FEATURE> admin page to admin-kit.

Users: <roles>
Main actions: <list/filter/create/edit/disable/export?>
Expected volume: <rows, growth>
Tenant/owner isolation: <none | owner id | tenant id>
Sort/filter requirements: <exact list>
Export/report requirements: <none | describe>
Acceptance examples: <3-5 cases>
```

Then the agent should fill the Access Pattern Sheet before touching code.

---

## Prompt for a review-only agent

```text
Review this admin-kit change against docs/EXTENDING-ADMIN-PAGES.md.

Check specifically:
- no generic query endpoint/filter DSL;
- no Scan/Count/offset pagination;
- every sort/filter maps to an index or is explicitly page-local;
- cursor includes all ExclusiveStartKey keys;
- GSI projection excludes secrets and large fields;
- server-side RBAC; no frontend-only authorization;
- non-GET x-amz-content-sha256 handling;
- tests/build/smoke evidence.

Report findings by severity. Do not edit files; return a checklist.
```
