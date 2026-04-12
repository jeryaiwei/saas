# Framework Observability v1.0 Compliance Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the observability primitive into compliance with `docs/framework/framework-observability-spec.md` v1.0 — by making `tenant_http` middleware open a root `http_request` span that inherits `request_id` / `method` / `path` (and gets `user_id` / `user_name` / `tenant_id` / `status` filled in by downstream middleware), then removing the 4 manual `Span::current().record("tenant_id", ...)` hacks in pagination v1.1, and filling out event-level + middleware instrument gaps identified in spec §12.

**Architecture:** The framework currently has `RequestContext` (task-local state) completely disconnected from tracing spans. `request_id` exists in the wire response but never appears in any log line, making client-reported bug reports untraceable. v1.0 closes this gap by having `tenant_http` create a root `info_span!("http_request", ...)` with `field::Empty` placeholders for `tenant_id` / `user_id` / `user_name` / `status`, then `.instrument(span)` over `next.run(req)`. `auth` middleware records user fields onto the current (root) span after session loads. Downstream `#[tracing::instrument]` spans inherit these fields automatically via tracing's span stack. The 4 pagination v1.1 manual `Span::current().record("tenant_id", ...)` hacks become obsolete and are removed. Middleware + infra layers that had zero `#[instrument]` coverage (`auth`, `tenant_guard`, `access::enforce`, `bcrypt::hash_password`) get a minimal `#[instrument(skip_all, name = "...")]` each so traces show the real cost breakdown of auth-heavy requests.

**Tech Stack:** Rust, axum 0.8, tracing 0.1 (with `Instrument` trait + `info_span!` macro + `field::Empty`), tower-http, tokio.

**Baseline assumption:** All previous spec-compliance work is merged:
- error-envelope v1.0
- pagination v1.0 + v1.1
- observability P0 fix (MatchedPath for metric path label)

Current test count is **179 passing**. Run `cd server-rs && cargo test --workspace 2>&1 | grep "test result:"` to confirm. If baseline differs, stop and investigate.

**Spec reference:** `docs/framework/framework-observability-spec.md` §5 (root span implementation contract) and §12 (gap table) are the primary drivers.

**Git policy:** Per standing user preference, **no automatic `git commit` steps**. The implementer must not run git commands. Each task ends with "report back for manual commit".

**Risk profile:** LOW-to-MEDIUM.
- Tasks 1-4 (root span + field inheritance) are **subtle** — tracing semantics are easy to get wrong. Plan explicitly tests that (a) `request_id` shows up in downstream service spans, and (b) the manual `record("tenant_id", ...)` removal doesn't regress test output
- Task 5-7 are **pure additions** (middleware `#[instrument]`, auth login `info!`, typo fix) — zero risk
- Wire contract **unchanged**
- Zero new crate dependencies (`tracing::Instrument` trait already in scope via workspace `tracing` dep)

**Expected test delta:** 179 → **180** (+1 new test for root span field capture; the other 6 tasks add no new tests because they're either pure attribute-additions, field removals, or verified by the existing smoke scripts).

---

### Task 1: Root span infrastructure — `tenant_http` creates `http_request` span

**Files:**
- Modify: `server-rs/crates/framework/src/middleware/tenant_http.rs`

Upgrades `tenant_http` from "create RequestContext + call next" to "create RequestContext + open root span + `.instrument(span)` over the context scope + record status after response". This is the single most impactful change — every downstream `#[instrument]` will automatically inherit `request_id` / `method` / `path` as ambient fields.

**Key tracing semantics** (must not confuse):
- `info_span!` creates a span but doesn't enter it
- `.instrument(span).await` enters the span for the duration of the future
- Fields declared with `field::Empty` can be `.record(...)` later when their value becomes known
- `.record` mutates the span; subscribers format the final value at event time

- [ ] **Step 1.1: Add imports and MatchedPath extraction**

Edit `crates/framework/src/middleware/tenant_http.rs`. Replace the `use` block and add the `MatchedPath` extraction:

```rust
use crate::context::{scope, RequestContext};
use crate::i18n;
use axum::{
    extract::{MatchedPath, Request},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use tracing::{field, info_span, Instrument};
```

- [ ] **Step 1.2: Rewrite `tenant_http` function body**

Replace the entire `tenant_http` function with:

```rust
pub async fn tenant_http(req: Request, next: Next) -> Response {
    let headers = req.headers();
    let request_id = extract_request_id(headers);
    let lang_code = extract_lang(headers);
    let tenant_id = headers
        .get("tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Capture method + route template for the root span. `MatchedPath`
    // has been populated by axum routing at this point (this middleware
    // is applied via `Router::layer(...)` which runs after routing).
    // Fall back to `<unmatched>` for pre-routing failures — same sentinel
    // used by `metrics_middleware` for cardinality safety.
    let method = req.method().as_str().to_owned();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "<unmatched>".to_string());

    // Open the request-scoped root span. All downstream #[instrument]
    // spans nest inside this one, so request_id / tenant_id / user_id
    // appear automatically in every log line without manual wiring.
    //
    // `tenant_id` / `user_id` / `user_name` / `status` start as
    // `field::Empty` — filled in later by `auth` middleware (user fields)
    // and by this middleware itself (`status` after `next.run` returns).
    let span = info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
        tenant_id = field::Empty,
        user_id = field::Empty,
        user_name = field::Empty,
        status = field::Empty,
    );

    // If the client provided a `tenant-id` header, record it immediately
    // on the root span so public routes (which skip auth) still have
    // tenant visibility in traces.
    if let Some(ref t) = tenant_id {
        span.record("tenant_id", t.as_str());
    }

    let ctx = RequestContext {
        request_id: Some(request_id),
        tenant_id,
        lang_code: Some(lang_code),
        ..Default::default()
    };

    async move {
        let response = scope(ctx, next.run(req)).await;
        // Record the final HTTP status onto the root span before it closes.
        tracing::Span::current().record("status", response.status().as_u16());
        response
    }
    .instrument(span)
    .await
}
```

**Why each piece**:
- `method` / `path` captured **before** the span macro because they need to be borrowed into the span formatting
- `tenant_id` header is recorded **inside** the span (via `span.record`) rather than inline in `info_span!` because it's `Option<String>`—inline `%tenant_id.as_deref().unwrap_or("")` is ugly and we prefer the `field::Empty` + `record` pattern uniformly
- `async move { ... }.instrument(span).await` pattern is mandatory—`scope(ctx, next.run(req)).instrument(span)` **would not compile** because `scope` consumes `ctx` and the future-pattern requires ownership

- [ ] **Step 1.3: Add a unit test for root span field capture**

Append to the existing `#[cfg(test)] mod tests` block in the same file:

```rust
    use tracing::{Dispatch, subscriber};
    use tracing_subscriber::{fmt, layer::SubscriberExt, registry, EnvFilter};
    use std::sync::{Arc, Mutex};
    use std::io;

    /// A tracing writer that buffers all output into a shared `Vec<u8>`
    /// so tests can inspect which span fields were emitted.
    #[derive(Clone)]
    struct TestWriter(Arc<Mutex<Vec<u8>>>);

    impl io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }

    #[tokio::test]
    async fn root_span_request_id_appears_in_downstream_event() {
        // Capture all tracing output into a buffer.
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer = {
            let c = captured.clone();
            move || -> TestWriter { TestWriter(c.clone()) }
        };
        let subscriber = registry()
            .with(EnvFilter::new("info"))
            .with(fmt::layer().with_writer(writer).with_ansi(false));
        let dispatch = Dispatch::new(subscriber);

        subscriber::with_default(dispatch, || {
            // Build a fixture span with the same shape tenant_http uses.
            let span = info_span!(
                "http_request",
                request_id = "req-fixture-123",
                method = "GET",
                path = "/users/{id}",
                tenant_id = field::Empty,
                user_id = field::Empty,
            );
            let _enter = span.enter();

            // Emit a downstream event as if from a handler. It must
            // carry the request_id field from the parent span.
            tracing::info!(target: "test", "downstream event from handler");
        });

        let output = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(
            output.contains("req-fixture-123"),
            "downstream event must inherit request_id from root span: {}",
            output
        );
        assert!(
            output.contains("/users/{id}"),
            "downstream event must inherit path template from root span: {}",
            output
        );
    }
```

- [ ] **Step 1.4: Run the new unit test**

```bash
cd server-rs && cargo test -p framework root_span_request_id 2>&1 | tail -15
```

Expected: 1 test passes. If it fails with "downstream event must inherit request_id" — root span field wiring is wrong, investigate before continuing.

- [ ] **Step 1.5: Full framework test suite**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result:"
```

Expected: framework tests at 96 (95 baseline + 1 new). If anything in the existing suite regresses (e.g. `lang_picks_first_tag` or other `tenant_http` helper tests), investigate — they should not be affected, but Step 1.2 changed the same file.

- [ ] **Step 1.6: Report Task 1 complete**

Report: tenant_http now opens `http_request` root span with `request_id` / `method` / `path` eager fields and `tenant_id` / `user_id` / `user_name` / `status` deferred fields. New unit test verifies downstream event inheritance. Framework tests 95 → 96.

---

### Task 2: Record `user_id` / `user_name` / `tenant_id` on root span from `auth` middleware

**Files:**
- Modify: `server-rs/crates/framework/src/middleware/auth.rs`

After `auth` successfully loads the Redis session, it already writes fields into `RequestContext::mutate(...)`. In v1.0 it must **also** `.record(...)` the same fields onto `Span::current()` (which is the root `http_request` span created by `tenant_http`).

- [ ] **Step 2.1: Add tracing Span to imports**

Edit `crates/framework/src/middleware/auth.rs`. The existing imports already bring `tracing` in via the indirect transitive — but to be explicit, add:

```rust
// Already present:
use crate::auth::{jwt, session, JwtClaims, UserSession};
// ...

// No new import needed — use `tracing::Span::current()` directly.
```

- [ ] **Step 2.2: Record session fields onto root span**

In the `auth` function, find the block:

```rust
    // 4. Populate RequestContext from the session
    RequestContext::mutate(|ctx: &mut RequestContext| {
        ctx.user_id = Some(user_session.user_id.clone());
        ctx.user_name = Some(user_session.user_name.clone());
        // ...
    });
```

Immediately **after** this block (before `req.extensions_mut().insert::<UserSession>(...)`), add:

```rust
    // 5. Propagate session identity to the root span so downstream
    //    log events carry `user_id` / `user_name` / `tenant_id`
    //    without every service function having to re-declare them.
    //    This is the sole reason spec §2.3 prohibits business code
    //    from manually `Span::current().record("tenant_id", ...)` —
    //    that job is done here, once, in the framework layer.
    let span = tracing::Span::current();
    span.record("user_id", user_session.user_id.as_str());
    span.record("user_name", user_session.user_name.as_str());
    if let Some(tid) = user_session.tenant_id.as_deref() {
        span.record("tenant_id", tid);
    }
```

**Note** the numbering comment: the existing block was labeled `4.`, and the next block is labeled `5.` — renumber that one to `6.` to keep the sequence intact:

```rust
    // 6. Stash session + claims in request extensions ...
```

- [ ] **Step 2.3: Compile-check**

```bash
cd server-rs && cargo check -p framework 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 2.4: Run framework + modules tests**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
```

Expected: all suites still pass (no new tests in this task; test count at 180 = 179 baseline + 1 from Task 1).

- [ ] **Step 2.5: Report Task 2 complete**

Report: auth middleware now records `user_id` / `user_name` / `tenant_id` onto the root span after session load. Downstream handlers / services / repos will see these fields inherited. No regressions in existing tests.

---

### Task 3: Remove manual `Span::current().record("tenant_id", ...)` from 4 `find_page` methods

**Files:**
- Modify: `server-rs/crates/modules/src/domain/user_repo.rs`
- Modify: `server-rs/crates/modules/src/domain/role_repo.rs`

Now that root span inherits `tenant_id` from `auth` middleware (Task 2), the 4 manual `.record("tenant_id", ...)` lines in pagination v1.1 are redundant. Delete them plus the `tenant_id = tracing::field::Empty,` line in each `#[tracing::instrument]` declaration.

Why the `field::Empty` declaration must **also** be removed: the root span already has `tenant_id` as one of its fields; declaring it again on a child span creates a **shadow** that hides the inherited value. Tracing fields are per-span, not merged across the parent chain — two spans with the same field name show the child's value (which would be `Empty` since we're removing the `.record`).

- [ ] **Step 3.1: Remove tenant_id Empty declaration + record from `user_repo::find_page`**

In `crates/modules/src/domain/user_repo.rs`, find the `find_page` method. The `#[tracing::instrument(...)]` block currently contains:

```rust
    #[tracing::instrument(skip_all, fields(
        tenant_id = tracing::field::Empty,
        has_user_name = filter.user_name.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
```

Change it to (remove the `tenant_id` line):

```rust
    #[tracing::instrument(skip_all, fields(
        has_user_name = filter.user_name.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
```

Then find the method body's tenant recording block:

```rust
        let tenant = current_tenant_scope();
        if let Some(t) = tenant.as_deref() {
            tracing::Span::current().record("tenant_id", t);
        }
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);
```

Change it to:

```rust
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);
```

(Removes the `if let Some(t) = tenant.as_deref() { ... }` block entirely. `tenant` is still needed as the SQL bind parameter.)

- [ ] **Step 3.2: Same change for `role_repo::find_page`**

In `crates/modules/src/domain/role_repo.rs`, locate the `find_page` method (the one returning `Page<SysRole>`, not `AllocatedUserRow`). Apply the same deletion:

- Remove `tenant_id = tracing::field::Empty,` from its `#[instrument(...)]` fields block
- Remove the `if let Some(t) = tenant.as_deref() { tracing::Span::current().record("tenant_id", t); }` block from the body

- [ ] **Step 3.3: Same change for `role_repo::find_allocated_users_page`**

Same file. Find `find_allocated_users_page`. Apply the same two deletions.

- [ ] **Step 3.4: Same change for `role_repo::find_unallocated_users_page`**

Same file. Find `find_unallocated_users_page`. Apply the same two deletions.

- [ ] **Step 3.5: Verify the 4 methods still compile and no tenant_id reference remains in these spans**

```bash
cd server-rs && cargo check -p modules 2>&1 | tail -10
cd server-rs && grep -n 'tenant_id.*field::Empty\|record("tenant_id"' crates/modules/src/domain/*.rs
```

First command: should be clean. Second command: should print **nothing** (no matches). If anything prints, a line was missed.

- [ ] **Step 3.6: Run modules tests**

```bash
cd server-rs && cargo test -p modules 2>&1 | grep "test result:"
```

Expected: all 4 test binaries pass (modules-lib 33, role-integration 23, user-integration 25, plus doc-tests). No regression.

- [ ] **Step 3.7: Report Task 3 complete**

Report: 4 `find_page` methods cleaned up — removed the manual `tenant_id` shadow that was hiding root span inheritance. grep-verified zero `record("tenant_id"` references left in modules.

---

### Task 4: Smoke verification — `request_id` actually shows up in logs

This is **not a code change**. It's a one-shot manual verification that the plumbing across Tasks 1-3 actually works end-to-end in a live server. Because tracing semantics are subtle and we have no integration test that captures subscriber output from a real axum request, the only reliable verification is to run the app and eyeball the log.

- [ ] **Step 4.1: Build + run app with debug tracing**

```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null
sleep 1
cargo build -p app 2>&1 | tail -3
RUST_LOG=info,framework=debug ./target/debug/app > /tmp/tea-rs-obs-v10.log 2>&1 &
APP_PID=$!
sleep 2
```

- [ ] **Step 4.2: Hit an authenticated endpoint with a known request_id**

```bash
BASE="http://127.0.0.1:18080/api/v1"

# Login first
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

# Request with a fixed x-request-id so we can grep for it
curl -sS -o /dev/null -X GET "$BASE/system/user/list?pageNum=1&pageSize=10" \
  -H "Authorization: Bearer $TOKEN" \
  -H "x-request-id: observability-v10-smoke"

kill $APP_PID 2>/dev/null
wait 2>/dev/null
```

- [ ] **Step 4.3: Verify the log contains `request_id` on a downstream event**

```bash
grep "observability-v10-smoke" /tmp/tea-rs-obs-v10.log | head -10
```

**Expected**: multiple lines containing `observability-v10-smoke` — at minimum the root span's log line plus any `#[instrument]` spans from auth/tenant/user_repo that emit events during the request. If **only** the root span line appears and no downstream events carry the id, Task 2 (`auth` recording) or the `.instrument(span)` wrapping in Task 1 is broken.

Additionally verify `user_id` (which is `admin`'s actual UUID) appears on at least one downstream line:

```bash
grep "observability-v10-smoke" /tmp/tea-rs-obs-v10.log | grep -o 'user_id[=":][^", }]*' | head
```

**Expected**: at least one `user_id=<uuid>` or `user_id:"<uuid>"` match (the exact format depends on whether the formatter is json or compact — both are acceptable).

- [ ] **Step 4.4: Record the evidence**

Copy 3-5 representative log lines out of `/tmp/tea-rs-obs-v10.log` into the task report, showing `request_id`, `user_id`, and `tenant_id` all present on a deep-stack event (ideally from `user_repo::find_page`).

- [ ] **Step 4.5: Report Task 4 complete**

Report: observed `request_id` / `user_id` / `tenant_id` on downstream events in a live request — plumbing works. Paste 3-5 log lines as evidence.

---

### Task 5: Middleware `#[instrument]` coverage

**Files:**
- Modify: `server-rs/crates/framework/src/middleware/auth.rs`
- Modify: `server-rs/crates/framework/src/middleware/tenant.rs`
- Modify: `server-rs/crates/framework/src/middleware/access.rs`

Three middleware functions currently have **zero** `#[instrument]` coverage. Add a minimal span to each so trace trees show the real cost breakdown of auth-heavy requests.

**Why this matters**: `auth` does JWT decode + 2 Redis round-trips + session fetch. On a slow Redis day, traces without instrument leave operators guessing where the 200ms went.

- [ ] **Step 5.1: Add `#[instrument]` to `auth::auth`**

In `crates/framework/src/middleware/auth.rs`, above the `pub async fn auth(...)` signature, add:

```rust
#[tracing::instrument(skip_all, name = "middleware.auth")]
pub async fn auth(
    State(state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
```

The `name = "middleware.auth"` override gives the span a stable dotted name for Grafana/Loki queries (instead of `auth::middleware::auth::auth` which is noisy). `skip_all` is mandatory to avoid `req: ?Request` being debug-formatted into the span.

- [ ] **Step 5.2: Add `#[instrument]` to `tenant::tenant_guard`**

In `crates/framework/src/middleware/tenant.rs`:

```rust
#[tracing::instrument(skip_all, name = "middleware.tenant_guard")]
pub async fn tenant_guard(
    State(state): State<TenantState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
```

- [ ] **Step 5.3: Add `#[instrument]` to `access::enforce`**

In `crates/framework/src/middleware/access.rs`:

```rust
#[tracing::instrument(skip_all, name = "middleware.access", fields(
    has_permission = spec.permission.is_some(),
    has_role = spec.role.is_some(),
    has_scope = spec.scope.is_some(),
))]
pub async fn enforce(
    State(spec): State<Arc<AccessSpec>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
```

The `has_*` booleans tell operators which gates each route required without leaking the actual permission string (those are fine in debug level but shouldn't be cardinality-heavy metadata in span fields).

- [ ] **Step 5.4: Compile-check + run tests**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 180 passing (unchanged), clippy clean. The existing `auth::whitelist_*` and `access::*` unit tests are not affected by attribute macros.

- [ ] **Step 5.5: Report Task 5 complete**

Report: 3 middleware functions now have `#[instrument]` with stable `name = "middleware.*"` dotted names. Clippy + tests clean.

---

### Task 6: `bcrypt::hash_password` instrument coverage

**Files:**
- Modify: `server-rs/crates/framework/src/infra/crypto.rs`

Bcrypt hashing takes ~100ms at cost 12 (production) — it's the single slowest framework operation during login. Currently zero tracing coverage.

- [ ] **Step 6.1: Read current state**

```bash
cd server-rs && cat crates/framework/src/infra/crypto.rs
```

Note the existing `hash_password` / `verify_password` / `hash_password_with_cost` function signatures. They're plain `fn` (not `async`) — `#[tracing::instrument]` works on sync functions too.

- [ ] **Step 6.2: Add instrument attribute**

Add above `pub fn hash_password(password: &str) -> anyhow::Result<String>`:

```rust
#[tracing::instrument(skip_all, name = "infra.crypto.hash_password")]
pub fn hash_password(password: &str) -> anyhow::Result<String> {
```

**Do not** add it to `verify_password` (called on every login attempt — hot path, but already fast < 50ms; span overhead noticeable) or `hash_password_with_cost` (test helper, irrelevant in prod).

`skip_all` ensures the raw password string is never captured.

- [ ] **Step 6.3: Compile-check + tests**

```bash
cd server-rs && cargo test -p framework infra::crypto 2>&1 | tail -10
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
```

Expected: existing crypto tests still pass, workspace at 180.

- [ ] **Step 6.4: Report Task 6 complete**

Report: `hash_password` has a `infra.crypto.hash_password` instrument span. Test count unchanged.

---

### Task 7: Auth login success event + typo fix

**Files:**
- Modify: `server-rs/crates/modules/src/auth/service.rs`

Two small cleanups:
1. Add a `tracing::info!` event at the end of successful login (spec §6.4 requirement — login success is one of the few legitimate `info!` events because it's a low-frequency audit-grade event, not a hot-path trace)
2. Fix the `tenant = %tid` typo at line ~65 (spec §2.1: should be `tenant_id`)

- [ ] **Step 7.1: Fix the tenant field typo**

In `crates/modules/src/auth/service.rs`, find:

```rust
            .inspect(|p| {
                tracing::debug!(
                    tenant = %tid,
                    count = p.len(),
                    "admin user granted all menu permissions"
                );
            })?,
```

Change `tenant = %tid` to `tenant_id = %tid` and `count = p.len()` to `perm_count = p.len()` (for consistency with other `_count` field naming like `role_count` / `user_count`).

- [ ] **Step 7.2: Add login success event**

Find the end of the `login` function body. Just before `Ok(LoginTokenResponseDto { ... })`, add:

```rust
    tracing::info!(
        username = %user.user_name,
        user_id = %user.user_id,
        "login success"
    );

    Ok(LoginTokenResponseDto {
```

**Why fields on the event** when `user_id` is already on the root span: the root span's `user_id` is set by `auth` middleware, which runs on **subsequent** authenticated requests. On the login request itself, the user is not yet authenticated when `tenant_http` creates the root span — `user_id` field on the root span is still `Empty` at this point. The explicit event field is the only place the login flow records which user logged in.

- [ ] **Step 7.3: Compile-check + run auth integration tests**

```bash
cd server-rs && cargo test -p modules login 2>&1 | tail -10
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
```

Expected: all tests pass, workspace at 180.

- [ ] **Step 7.4: Run smoke scripts**

```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null
sleep 1
cargo build -p app 2>&1 | tail -3
./target/debug/app > /tmp/tea-rs-obs-smoke1.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null
wait 2>/dev/null

./target/debug/app > /tmp/tea-rs-obs-smoke2.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-user-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null
wait 2>/dev/null
```

Expected: `ALL 14 STEPS PASSED` + `ALL 16 STEPS PASSED`. Also grep for the login success event in the logs:

```bash
grep "login success" /tmp/tea-rs-obs-smoke1.log /tmp/tea-rs-obs-smoke2.log
```

Expected: at least 2 matches (one per smoke run's initial login).

- [ ] **Step 7.5: Report Task 7 complete**

Report: typo fix applied, login success info event emitted and observed in smoke logs, smoke 14/14 + 16/16 green.

---

### Task 8: Update spec gap table

**Files:**
- Modify: `server-rs/docs/framework/framework-observability-spec.md`

- [ ] **Step 8.1: Update §12 gap table**

Edit `server-rs/docs/framework/framework-observability-spec.md` §12. Change each row's status:

| 规范条目 | 之前状态 | 落地任务 | 新状态 |
|---|---|---|---|
| §3.2 path label `MatchedPath` | ✅ 已完成 | — (done 2026-04-12 P0) | ✅ 完成 |
| §2.3 root span 自动注入 | v0 没有 | Task 1 | ✅ v1.0 |
| §2.3 废除手写 `Span::current().record("tenant_id")` | 4 处 | Task 3 | ✅ v1.0 |
| §4.4 auth/tenant/access 补 `#[instrument]` | 0/3 | Task 5 | ✅ v1.0 |
| §4.5 `bcrypt::hash_password` instrument | 无 | Task 6 | ✅ v1.0 |
| §6.4 auth login 成功 `info!` event | 无 | Task 7 | ✅ v1.0 |
| §2.1 `tenant = %tid` typo 修复 | 1 处 | Task 7 | ✅ v1.0 |

Find the "v1.0 必做" / "v1.0 次做" sections below the table and replace them with:

```markdown
**v1.0 已全部完成**（2026-04-12）：7/11 gap 已实施，其余 4 项按触发器条件延期：

- ⏳ 业务 metric 首次埋点 → v1.2
- ⏳ OTLP / OpenTelemetry → v2.0
- ⏳ Runtime log level reload → v2.1
- ⏳ Automatic cardinality audit → v2.2
```

- [ ] **Step 8.2: Cross-reference update in pagination spec**

Edit `server-rs/docs/framework/framework-pagination-spec.md` §6.1 (the observability standard fields section). The observability contract now mandates that `tenant_id` comes from the root span — pagination spec's `find_page` `#[instrument]` no longer needs to declare it. Find the text that says to declare 5 standard fields and update it so it says 4 (`tenant_id` moved to root span responsibility).

Locate the "标准字段（每个 `find_page` 必须发出）" table. Change the `tenant_id` row's "来源" from `current_tenant_scope()` to `"root span (auto-inherited from tenant_http + auth middleware)"` and add a footnote: `**不得** 在 find_page 的 instrument 上重复声明该字段 (obs spec §2.3)`.

- [ ] **Step 8.3: Run final workspace verify**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo fmt --check && echo "fmt ok"
```

Expected:
- framework: 96 passed (95 baseline + 1 root span test from Task 1)
- workspace: 180 passed (179 baseline + 1)
- clippy clean
- fmt ok

- [ ] **Step 8.4: Report plan complete**

Report back with final summary:

- All 8 tasks complete
- Workspace tests: 179 → 180 (+1 new test for root span field capture)
- Framework tests: 95 → 96 (+1)
- Root span now carries `request_id` / `tenant_id` / `user_id` / `user_name` / `method` / `path` / `status`
- 4 manual `Span::current().record("tenant_id", ...)` sites removed
- 3 middleware functions + `bcrypt::hash_password` got `#[instrument]`
- Login success event now emitted at `info!` level
- `tenant = %tid` typo fixed
- Smoke tests 14/14 + 16/16 green
- Live log evidence captured in Task 4 showing `request_id` inheritance
- Zero wire contract changes
- Zero new crate dependencies
- Spec + pagination spec cross-reference updated

---

## Post-plan status snapshot

After all 8 tasks:

| Metric | v0 baseline | v1.0 target |
|---|---|---|
| Total tests passing | 179 | 180 (+1) |
| Framework tests | 95 | 96 (+1) |
| Root span with request_id | ❌ | ✅ |
| Downstream events inherit context | ❌ (manual 4 places) | ✅ (automatic) |
| Middleware instrument coverage (auth/tenant/access) | 0/3 | 3/3 |
| `bcrypt::hash_password` instrument | ❌ | ✅ |
| Login success info event | ❌ | ✅ |
| Wire contract changes | — | 0 |
| New runtime deps | — | 0 |
| New dev deps | — | 0 |
| Smoke tests | 14/14 + 16/16 | 14/14 + 16/16 |

---

## What this plan explicitly doesn't do (deferred per spec §11)

1. **Business metrics first埋点** — deferred to v1.2. Spec §2.6 defines the 5 starter templates but埋点 belongs in each module's own plan
2. **`tracing-opentelemetry` / OTLP** — v2.0, triggered by second service or mesh
3. **`/admin/log-level` runtime reload** — v2.1, triggered by a prod debug incident
4. **Automatic cardinality audit** — v2.2, triggered by Prometheus OOM
5. **Structured audit log sink** — v3.0, triggered by compliance review
6. **infra/pg.rs + infra/redis.rs instrument coverage** — deferred; connection pool operations are already covered by sqlx's built-in `tracing` feature at debug level
7. **i18n::get_message instrument** — explicitly rejected by spec §4.5 (over-instrumentation on hot path)

---

## Execution handoff

Plan complete. Two execution options:

**1. Subagent-Driven (recommended)** — Controller dispatches fresh implementer + spec-reviewer + code-quality-reviewer subagents per task. Each task gets isolated context, two-stage review.

**2. Inline Execution** — Execute tasks in the current session with checkpoints between tasks for user review.

Which approach?
