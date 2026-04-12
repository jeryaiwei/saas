# Framework Error + Envelope v1.0 Compliance Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the error + response envelope primitive into compliance with `docs/framework-error-envelope-spec.md` v1.0 — by unifying the two duplicate wire structs, deleting three confirmed dead capabilities, removing the implicit `From<anyhow::Error>` path, consolidating the status-code mapping into a single source, adding i18n coverage + wire regression tests.

**Architecture:** The framework currently has two near-identical response body structs (`ApiResponse<T>` and `ErrorBody`), three dead methods/fields (`ApiResponse::with_code`, `AppError::business_with_params`, `AppError::Business.{params,data}`), and an implicit `#[from] anyhow::Error` conversion that hides the "this is now a 500" decision behind `?`. v1.0 removes all of these, replaces `ErrorBody` with `ApiResponse<serde_json::Value>` so the wire contract has exactly one source of truth, and adds two new framework-level tests (i18n coverage + wire regression table-driven) so future drift is caught at CI time.

**Tech Stack:** Rust, axum 0.8, thiserror 1.x, serde_json, tracing, anyhow, validator 0.20.

**Baseline assumption:** All hygiene-pass and pagination v1.0 changes are already merged. Current test count is **153 passing**. Run `cd server-rs && cargo test --workspace 2>&1 | grep "test result"` before starting. If baseline is different, stop and investigate.

**Spec reference:** `docs/framework-error-envelope-spec.md` §12 — the gap table drives every task in this plan.

**Git policy:** Per standing user preference, **no automatic `git commit` steps**. The implementer must not run git commands. Each task ends with "report back for manual commit". The user decides commit scope.

**Risk profile:** LOW. Pre-plan grep confirmed:
- `ApiResponse::with_code` has 0 external callers (dead)
- `AppError::business_with_params` has 0 callers (dead)
- `AppError::Business { data: Some(...) }` has 0 occurrences (dead)
- `AppError::Internal` `#[from]` conversion has 0 implicit users (service layer uses explicit `.into_internal()`, repos stay in `anyhow`, framework internals construct `AppError::Internal(anyhow::anyhow!(...))` directly)

All deletions are confirmed dead-code removal, not behavior changes. Wire contract for non-error responses stays identical byte-for-byte; error wire shape gets slightly cleaner (camelCase `requestId` handling via `ApiResponse` serde attrs instead of hand-written `ErrorBody` attrs, but output JSON is bit-for-bit identical).

---

### Task 1: Delete dead constructors (`with_code` + `business_with_params`)

**Files:**
- Modify: `server-rs/crates/framework/src/response/envelope.rs`
- Modify: `server-rs/crates/framework/src/error/app_error.rs`

Deletes two confirmed-dead public constructors. No callers, no tests needed — the absence is the verification.

- [ ] **Step 1.1: Delete `ApiResponse::with_code`**

Edit `crates/framework/src/response/envelope.rs`. Remove the entire `pub fn with_code(...)` function body. Update `ApiResponse::ok` to inline what `with_code` used to do (look up SUCCESS message, populate request_id, populate timestamp). Update `ApiResponse::success` similarly.

Replace lines 33-54 (the entire `impl<T> ApiResponse<T>` block and `impl ApiResponse<()>` block) with:

```rust
impl<T> ApiResponse<T> {
    /// Wrap `data` with a 200 SUCCESS envelope. The only way to
    /// construct a successful response — error responses must go
    /// through `AppError::IntoResponse`.
    pub fn ok(data: T) -> Self {
        let lang = RequestContext::with_current(|c| c.lang_code.clone())
            .flatten()
            .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string());
        let request_id = RequestContext::with_current(|c| c.request_id.clone()).flatten();
        Self {
            code: ResponseCode::SUCCESS.as_i32(),
            msg: i18n::get_message(ResponseCode::SUCCESS, &lang),
            data: Some(data),
            request_id,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}

impl ApiResponse<()> {
    /// Success envelope with no payload (create/update/delete endpoints).
    pub fn success() -> Self {
        let lang = RequestContext::with_current(|c| c.lang_code.clone())
            .flatten()
            .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string());
        let request_id = RequestContext::with_current(|c| c.request_id.clone()).flatten();
        Self {
            code: ResponseCode::SUCCESS.as_i32(),
            msg: i18n::get_message(ResponseCode::SUCCESS, &lang),
            data: None,
            request_id,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}
```

There's now some duplication between `ok` and `success` — that's fine for v1.0 (2 copies), we'll DRY it in Task 3 once `AppError::IntoResponse` joins the envelope.

- [ ] **Step 1.2: Delete `AppError::business_with_params`**

Edit `crates/framework/src/error/app_error.rs`. Remove the method entirely (lines 73-79 in current file):

```rust
// DELETE these lines:
pub fn business_with_params(code: ResponseCode, params: HashMap<String, String>) -> Self {
    Self::Business {
        code,
        params: Some(params),
        data: None,
    }
}
```

The `business(code)` constructor stays.

- [ ] **Step 1.3: Compile-check**

```bash
cd server-rs && cargo check -p framework 2>&1 | tail -10
```

Expected: clean. If `std::collections::HashMap` is now unused in `app_error.rs`, clippy will warn in a later step — that's fine, handled in Task 2.

- [ ] **Step 1.4: Run framework tests**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
```

Expected: 69 passed (unchanged — nothing tests the deleted methods).

- [ ] **Step 1.5: Report Task 1 complete**

Two dead constructors removed. Framework compiles clean, tests unchanged. Await user's commit decision.

---

### Task 2: Delete dead `Business` variant fields (`params`, `data`)

**Files:**
- Modify: `server-rs/crates/framework/src/error/app_error.rs`

Removes the `params: Option<HashMap<String, String>>` and `data: Option<Value>` fields from `AppError::Business` — confirmed zero callers populate either with `Some(...)`. Simplifies the variant to `Business { code: ResponseCode }`.

- [ ] **Step 2.1: Simplify the `Business` variant definition**

Edit `crates/framework/src/error/app_error.rs`. Change the enum variant from:

```rust
#[error("business error [{code}]")]
Business {
    code: ResponseCode,
    params: Option<HashMap<String, String>>,
    data: Option<Value>,
},
```

to:

```rust
#[error("business error [{code}]")]
Business { code: ResponseCode },
```

- [ ] **Step 2.2: Simplify the `business` constructor**

In the same file, change the `business` constructor from:

```rust
pub fn business(code: ResponseCode) -> Self {
    Self::Business {
        code,
        params: None,
        data: None,
    }
}
```

to:

```rust
pub fn business(code: ResponseCode) -> Self {
    Self::Business { code }
}
```

- [ ] **Step 2.3: Simplify the `IntoResponse` match arm for Business**

In the same file, the Business arm in `IntoResponse::into_response`'s match currently looks like:

```rust
AppError::Business { code, params, data } => {
    let msg = match &params {
        Some(p) => {
            let params_ref: HashMap<&str, String> =
                p.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            i18n::get_message_with_params(code, &lang, &params_ref)
        }
        None => i18n::get_message(code, &lang),
    };
    (code.as_i32(), msg, data.unwrap_or(Value::Null))
}
```

Replace with:

```rust
AppError::Business { code } => {
    (code.as_i32(), i18n::get_message(code, &lang), Value::Null)
}
```

- [ ] **Step 2.4: Clean up now-unused imports**

At the top of `app_error.rs`, the following imports may now be unused:

```rust
use std::collections::HashMap;   // ← if no other usage
use serde_json::Value;            // ← still used by Validation and ErrorBody paths
```

Run `cargo check -p framework 2>&1` and let the compiler tell you. Remove only the ones flagged as unused.

- [ ] **Step 2.5: Verify framework compiles + tests still pass**

```bash
cd server-rs && cargo check -p framework 2>&1 | tail -10
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo clippy -p framework --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 69 tests passed, clippy clean.

- [ ] **Step 2.6: Verify downstream (modules, app) still compiles**

```bash
cd server-rs && cargo check --workspace 2>&1 | tail -10
```

Expected: clean. Any module/app code that destructured `AppError::Business { code, params, data }` would fail here. The grep during plan drafting confirmed zero such destructuring exists — this step is the definitive proof.

- [ ] **Step 2.7: Report Task 2 complete**

`AppError::Business` simplified from 3-field variant to 1-field variant. Framework + modules + app all compile clean.

---

### Task 3: Merge wire envelope — delete `ErrorBody`, use `ApiResponse<Value>` in `IntoResponse`

**Files:**
- Modify: `server-rs/crates/framework/src/error/app_error.rs`
- Modify: `server-rs/crates/framework/src/response/envelope.rs` (add one helper method)

Removes the `ErrorBody` struct entirely and makes `AppError::IntoResponse` build an `ApiResponse<serde_json::Value>`. Wire output is byte-for-byte identical; the difference is that there's now **one** serde struct responsible for the wire shape, eliminating drift risk.

- [ ] **Step 3.1: Add `ApiResponse::error` constructor helper**

Edit `crates/framework/src/response/envelope.rs`. Add a new `pub(crate)` constructor dedicated to the error path. This constructor is **not** part of the public API — only `AppError::IntoResponse` uses it. This keeps the "ok/success are the only public constructors" rule intact.

Add to `impl ApiResponse<serde_json::Value>`:

```rust
impl ApiResponse<serde_json::Value> {
    /// Error envelope used exclusively by `AppError::IntoResponse`.
    /// Not for handler/service use — those paths return
    /// `Result<ApiResponse<T>, AppError>`, and the error branch is
    /// serialized through this helper automatically.
    ///
    /// `data` is `serde_json::Value` so the Validation variant can
    /// carry the list of `FieldError`s and other variants can carry
    /// `Value::Null`.
    pub(crate) fn error(code: ResponseCode, msg: String, data: serde_json::Value) -> Self {
        let request_id = RequestContext::with_current(|c| c.request_id.clone()).flatten();
        Self {
            code: code.as_i32(),
            msg,
            data: Some(data),
            request_id,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}
```

Note: `data: Some(Value::Null)` serializes as `"data": null` — identical to the previous `ErrorBody` where `data: Value` and `Null` serialized as `"data": null`. Wire compatible.

- [ ] **Step 3.2: Replace `ErrorBody` with `ApiResponse<Value>` in `IntoResponse`**

Edit `crates/framework/src/error/app_error.rs`. Delete the entire `ErrorBody` struct definition (lines 53-62). Rewrite the `IntoResponse for AppError` impl to use `ApiResponse::<serde_json::Value>::error(...)`:

```rust
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        use crate::response::ApiResponse;

        let lang = RequestContext::with_current(|c| c.lang_code.clone())
            .flatten()
            .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string());
        let status = self.status_code();

        let (code, msg, data) = match self {
            AppError::Business { code } => {
                (code, i18n::get_message(code, &lang), Value::Null)
            }
            AppError::Auth { code } => {
                (code, i18n::get_message(code, &lang), Value::Null)
            }
            AppError::Forbidden { code } => {
                (code, i18n::get_message(code, &lang), Value::Null)
            }
            AppError::Validation { errors } => {
                let translated: Vec<FieldError> = errors
                    .into_iter()
                    .map(|e| {
                        let key = format!("valid.{}", e.message);
                        let message = match i18n::get_by_key(&key, &lang) {
                            Some(m) => m,
                            None => {
                                tracing::warn!(
                                    i18n_key = %key,
                                    lang = %lang,
                                    "missing i18n entry for validation error; falling back to raw code"
                                );
                                e.message
                            }
                        };
                        FieldError {
                            field: e.field,
                            message,
                        }
                    })
                    .collect();
                (
                    ResponseCode::BAD_REQUEST,
                    i18n::get_message(ResponseCode::BAD_REQUEST, &lang),
                    serde_json::to_value(&translated).unwrap_or(Value::Null),
                )
            }
            AppError::Internal(ref e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    ResponseCode::INTERNAL_SERVER_ERROR,
                    i18n::get_message(ResponseCode::INTERNAL_SERVER_ERROR, &lang),
                    Value::Null,
                )
            }
        };

        let body = ApiResponse::<Value>::error(code, msg, data);
        (status, Json(body)).into_response()
    }
}
```

Key differences from the pre-v1.0 version:
1. `ErrorBody` struct is gone — `ApiResponse<Value>` is the single wire struct
2. The match arms produce `(ResponseCode, String, Value)` tuples instead of `(i32, String, Value)` — `ApiResponse::error` takes `ResponseCode` and calls `as_i32()` internally
3. `request_id` and `timestamp` are now set inside `ApiResponse::error`, not inline in the match

- [ ] **Step 3.3: Delete the `ErrorBody` struct**

In the same file, remove lines 53-62 (the `#[derive(Debug, Serialize)] struct ErrorBody { ... }` block). After deletion, run:

```bash
cd server-rs && cargo check -p framework 2>&1 | tail -15
```

Expected: clean compile. Any remaining import of `Serialize` that was only used by `ErrorBody` should now be flagged unused — remove it. `chrono::SecondsFormat` / `Utc` may also become unused if they were only used in the old inline path; remove if unused.

- [ ] **Step 3.4: Verify framework tests + existing wire behavior unchanged**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo clippy -p framework --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo test --workspace 2>&1 | grep "test result"
```

Expected:
- framework: 69 passed
- workspace: 153 passed (unchanged)
- clippy clean

The status_mapping test in `app_error.rs::tests` still passes (it doesn't look at body shape).

- [ ] **Step 3.5: Manual wire-shape verification (optional but recommended)**

Run the smoke tests to confirm no wire drift:

```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null
sleep 1
cargo build -p app 2>&1 | tail -3
./target/debug/app > /tmp/tea-rs-err-v10.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
bash scripts/smoke-user-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null
wait 2>/dev/null
```

Expected: 14/14 + 16/16 green. Smoke tests include the "invalid request → 400" path and "missing record → 1001" path, both of which exercise `AppError::IntoResponse`.

- [ ] **Step 3.6: Report Task 3 complete**

`ErrorBody` deleted, `AppError::IntoResponse` now uses `ApiResponse::<Value>::error(...)`. Single wire struct across all response paths. Smoke tests green.

---

### Task 4: Remove `#[from] anyhow::Error` from `AppError::Internal`

**Files:**
- Modify: `server-rs/crates/framework/src/error/app_error.rs`

Removes the implicit `From<anyhow::Error>` path. Pre-plan grep confirmed zero callers depend on this — service layer already uses `.into_internal()` explicitly (55 sites), repo layer stays in `anyhow::Result`, framework internals use direct variant construction. This task is therefore zero-risk; the `#[from]` is a latent hazard, not an active dependency.

- [ ] **Step 4.1: Remove the `#[from]` attribute**

Edit `crates/framework/src/error/app_error.rs`. Change:

```rust
#[error(transparent)]
Internal(#[from] anyhow::Error),
```

to:

```rust
#[error(transparent)]
Internal(anyhow::Error),
```

- [ ] **Step 4.2: Verify nothing relies on the implicit conversion**

```bash
cd server-rs && cargo check --workspace 2>&1 | tail -15
```

Expected: clean. If any call site fails here with `cannot convert anyhow::Error to AppError via ?`, that site needs `.into_internal()` added. Grep pre-verified zero such sites, so this step is a sanity check.

- [ ] **Step 4.3: Run all tests**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 153 passed, clippy clean.

- [ ] **Step 4.4: Report Task 4 complete**

`#[from]` removed. The explicit `.into_internal()` path is now the only way to turn `anyhow::Error` into `AppError::Internal`. This makes "when does a business error become a 500?" a visible decision at every call site.

---

### Task 5: Consolidate status-code mapping into a single source

**Files:**
- Modify: `server-rs/crates/framework/src/error/app_error.rs`

The current code has the variant-to-HTTP-status mapping in three places:
1. `fn status_code()` — match statement
2. `impl IntoResponse` — `self.status_code()` call (inherits from #1)
3. `mod tests::status_mapping` — hand-written assertions

Adding a 6th variant would require editing all three. Consolidate to a **single const method per variant** via a private helper, and rewrite the test to iterate a static list.

- [ ] **Step 5.1: Write the failing (iterating) status_mapping test**

Replace the existing `status_mapping` test with a table-driven version:

```rust
#[test]
fn status_mapping_covers_every_variant() {
    // Static table: every AppError variant MUST have an entry here.
    // Adding a new variant without updating this table will cause a
    // compile failure in `exhaustive_check` below.
    let cases: &[(AppError, StatusCode)] = &[
        (
            AppError::business(ResponseCode::DATA_NOT_FOUND),
            StatusCode::OK,
        ),
        (
            AppError::auth(ResponseCode::TOKEN_INVALID),
            StatusCode::UNAUTHORIZED,
        ),
        (
            AppError::forbidden(ResponseCode::FORBIDDEN),
            StatusCode::FORBIDDEN,
        ),
        (
            AppError::Validation { errors: vec![] },
            StatusCode::BAD_REQUEST,
        ),
        (
            AppError::Internal(anyhow::anyhow!("boom")),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];

    for (err, expected) in cases {
        assert_eq!(
            err.status_code(),
            *expected,
            "status mapping drift for {:?}",
            err
        );
    }

    // Compile-time exhaustive check: if a new variant is added to
    // AppError without extending the `cases` table OR the
    // `status_code` match, one of these will fail.
    fn exhaustive_check(e: &AppError) -> StatusCode {
        match e {
            AppError::Business { .. } => StatusCode::OK,
            AppError::Auth { .. } => StatusCode::UNAUTHORIZED,
            AppError::Forbidden { .. } => StatusCode::FORBIDDEN,
            AppError::Validation { .. } => StatusCode::BAD_REQUEST,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    for (err, expected) in cases {
        assert_eq!(exhaustive_check(err), *expected);
    }
}
```

- [ ] **Step 5.2: Run test to verify it passes (it should, since status_code() already works)**

```bash
cd server-rs && cargo test -p framework status_mapping_covers_every_variant 2>&1 | tail -10
```

Expected: pass. This test replaces the old `status_mapping` test — delete the old one.

- [ ] **Step 5.3: Optional simplification — leave `status_code()` as-is**

The spec §10 禁止模式 table says "paginate_with_tracing / ListQuery<F,S,P> < 6 调用点时过度抽象". 5 variants in `status_code()` is below the abstraction threshold; the match is clearer than a const table. **Keep the match.** The exhaustive_check in the test is the compile-time guarantee that we don't drift.

This step is intentionally a no-op: it documents the decision NOT to refactor `status_code()`.

- [ ] **Step 5.4: Run framework tests**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
```

Expected: 69 passed (old status_mapping test replaced by status_mapping_covers_every_variant — count unchanged).

- [ ] **Step 5.5: Report Task 5 complete**

Status mapping now has a single declarative source in `status_code()` + a table-driven + exhaustive-check test. Adding a 6th variant requires updating only 2 places (the match + the cases table) with a compile-time reminder via `exhaustive_check`.

---

### Task 6: i18n coverage test — every ResponseCode has entries in zh-CN and en-US

**Files:**
- Modify: `server-rs/crates/framework/src/i18n/mod.rs`

Per spec §5.3: new test asserting every `ResponseCode::*` constant has an i18n entry in both languages. Uses a hand-written explicit list (spec forbids reflection/build-script discovery — the manual list is the "enforcement reminder").

- [ ] **Step 6.1: Write the failing test**

Add to `crates/framework/src/i18n/mod.rs` `mod tests`:

```rust
    #[test]
    fn every_response_code_has_i18n_entries_in_all_langs() {
        // Explicit list of every ResponseCode constant defined in
        // `framework/src/response/codes.rs`. When adding a new const,
        // YOU MUST add it here too AND add matching entries to both
        // i18n JSON files. See spec §5.3.
        let codes = &[
            // HTTP-aligned
            ResponseCode::SUCCESS,
            ResponseCode::BAD_REQUEST,
            ResponseCode::UNAUTHORIZED,
            ResponseCode::FORBIDDEN,
            ResponseCode::TOO_MANY_REQUESTS,
            ResponseCode::INTERNAL_SERVER_ERROR,
            // 1000-1029 general business
            ResponseCode::PARAM_INVALID,
            ResponseCode::DATA_NOT_FOUND,
            ResponseCode::DUPLICATE_KEY,
            ResponseCode::OPTIMISTIC_LOCK_CONFLICT,
            ResponseCode::OPERATION_NOT_ALLOWED,
            // 2000-2039 auth
            ResponseCode::TOKEN_INVALID,
            ResponseCode::TOKEN_EXPIRED,
            ResponseCode::ACCOUNT_LOCKED,
            ResponseCode::CAPTCHA_INVALID,
            // 3000-3029 user
            ResponseCode::USER_NOT_FOUND,
            ResponseCode::INVALID_CREDENTIALS,
            // 4000-4029 tenant
            ResponseCode::TENANT_DISABLED,
            ResponseCode::TENANT_EXPIRED,
        ];

        for code in codes {
            for lang in ["zh-CN", "en-US"] {
                let msg = get_message(*code, lang);
                assert!(
                    !msg.starts_with('['),
                    "missing i18n entry for {:?} in {}: got fallback sentinel {:?}",
                    code,
                    lang,
                    msg
                );
                assert!(
                    !msg.is_empty(),
                    "empty i18n entry for {:?} in {}",
                    code,
                    lang
                );
            }
        }
    }
```

- [ ] **Step 6.2: Run test to verify it passes OR fails with a specific missing entry**

```bash
cd server-rs && cargo test -p framework every_response_code_has_i18n_entries 2>&1 | tail -20
```

Expected: **passes** on current code (all 19 constants should already have entries because the codebase currently uses them in real paths that would fail validation if i18n was missing).

If it fails: look at which code+lang combination is missing, add the entry to the JSON file, re-run. The test itself is the truth.

- [ ] **Step 6.3: Run full framework test suite**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
```

Expected: framework tests at 70 (69 + 1 new test).

- [ ] **Step 6.4: Report Task 6 complete**

i18n coverage test in place. Adding a new ResponseCode now requires updating 4 files in sync: `codes.rs` (constant), `zh-CN.json`, `en-US.json`, and the test's `codes` array. The test is the CI enforcement point; the spec is the human-facing rule.

---

### Task 7: Wire regression test — table-driven serialization shape assertions

**Files:**
- Create: `server-rs/crates/framework/src/response/wire_test.rs`
- Modify: `server-rs/crates/framework/src/response/mod.rs` (add `#[cfg(test)] mod wire_test;`)

Adds a comprehensive wire shape regression test covering every `ApiResponse` / `AppError` response path. Asserts serde output has exactly `code/msg/data/requestId?/timestamp` in camelCase, `requestId` is skipped when None, `timestamp` matches RFC3339 format, etc. This is the **CI enforcement** for spec §2.1's "single wire envelope" rule.

- [ ] **Step 7.1: Create the wire regression test file**

Create `crates/framework/src/response/wire_test.rs`:

```rust
//! Wire-shape regression tests for `ApiResponse<T>` and `AppError::IntoResponse`.
//!
//! These tests assert the serialized JSON key set, ordering, and camelCase
//! conventions for every response path. They are the CI enforcement of
//! spec §2.1 (single wire envelope) — a PR that accidentally renames
//! `msg` to `message` or adds a new top-level field without coordination
//! will fail here, not in production.

#![cfg(test)]

use super::{ApiResponse, ResponseCode};
use crate::error::{AppError, FieldError};
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use serde_json::Value;

/// Block-on helper: drain an axum response body into a serde_json::Value.
fn body_as_json(resp: axum::response::Response) -> Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (_, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    })
}

/// Assert the top-level keys of a wire response match the spec §2.1
/// contract: `code` + `msg` + `data` + (optional) `requestId` + `timestamp`.
/// Fails if any extra field appears or a required field is missing.
fn assert_wire_shape(value: &Value, expects_request_id: bool) {
    let obj = value.as_object().expect("response body must be a JSON object");

    // Required keys
    assert!(obj.contains_key("code"), "missing `code`: {:?}", obj);
    assert!(obj.contains_key("msg"), "missing `msg`: {:?}", obj);
    assert!(obj.contains_key("data"), "missing `data`: {:?}", obj);
    assert!(obj.contains_key("timestamp"), "missing `timestamp`: {:?}", obj);

    // requestId is conditionally present
    if expects_request_id {
        assert!(
            obj.contains_key("requestId"),
            "expected `requestId` to be present: {:?}",
            obj
        );
    } else {
        assert!(
            !obj.contains_key("requestId"),
            "`requestId` should be skipped when None: {:?}",
            obj
        );
    }

    // No extra top-level fields allowed
    let allowed = ["code", "msg", "data", "requestId", "timestamp"];
    for k in obj.keys() {
        assert!(
            allowed.contains(&k.as_str()),
            "unexpected top-level field `{}` in response: {:?}",
            k,
            obj
        );
    }

    // Forbidden legacy / alternative spellings
    assert!(
        !obj.contains_key("message"),
        "wire uses `msg`, not `message`: {:?}",
        obj
    );
    assert!(
        !obj.contains_key("request_id"),
        "wire uses camelCase `requestId`, not snake_case: {:?}",
        obj
    );

    // `code` must be an integer
    assert!(obj["code"].is_i64(), "`code` must be an integer: {:?}", obj);

    // `msg` must be a string
    assert!(obj["msg"].is_string(), "`msg` must be a string: {:?}", obj);

    // `timestamp` must be an RFC3339 string with millisecond precision
    let ts = obj["timestamp"].as_str().expect("`timestamp` must be a string");
    // Shape check: YYYY-MM-DDTHH:MM:SS.sssZ (24 chars)
    assert_eq!(ts.len(), 24, "timestamp must be 24 chars (millisecond RFC3339): {:?}", ts);
    assert!(ts.ends_with('Z'), "timestamp must be UTC (end with Z): {:?}", ts);
}

#[test]
fn success_wire_shape_with_data() {
    let resp = ApiResponse::ok(42_i64).into_response();
    let json = body_as_json(resp);
    assert_wire_shape(&json, false); // no RequestContext → no requestId
    assert_eq!(json["code"], 200);
    assert_eq!(json["data"], 42);
}

#[test]
fn success_wire_shape_no_data() {
    let resp = ApiResponse::success().into_response();
    let json = body_as_json(resp);
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 200);
    assert_eq!(json["data"], Value::Null);
}

#[test]
fn business_error_wire_shape() {
    let err = AppError::business(ResponseCode::DATA_NOT_FOUND);
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 1001);
    assert_eq!(json["data"], Value::Null);
}

#[test]
fn auth_error_wire_shape() {
    let err = AppError::auth(ResponseCode::TOKEN_INVALID);
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 2001);
    assert_eq!(json["data"], Value::Null);
}

#[test]
fn forbidden_error_wire_shape() {
    let err = AppError::forbidden(ResponseCode::FORBIDDEN);
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 403);
    assert_eq!(json["data"], Value::Null);
}

#[test]
fn validation_error_wire_shape_carries_field_list() {
    let err = AppError::Validation {
        errors: vec![
            FieldError {
                field: "user_name".into(),
                message: "length".into(),
            },
            FieldError {
                field: "page.page_num".into(),
                message: "range".into(),
            },
        ],
    };
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 400);

    // `data` is an array of FieldError-shaped objects
    let data = json["data"].as_array().expect("validation data must be array");
    assert_eq!(data.len(), 2);
    for item in data {
        let obj = item.as_object().unwrap();
        assert!(obj.contains_key("field"));
        assert!(obj.contains_key("message"));
        // No other fields on FieldError
        assert_eq!(obj.len(), 2, "FieldError must have exactly 2 keys: {:?}", obj);
    }
}

#[test]
fn internal_error_wire_shape() {
    let err = AppError::Internal(anyhow::anyhow!("database unreachable"));
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 500);
    assert_eq!(json["data"], Value::Null);
    // msg must NOT contain the raw anyhow error text (that's logged, not wire)
    let msg = json["msg"].as_str().unwrap();
    assert!(
        !msg.contains("database unreachable"),
        "internal error wire msg must not leak raw error details: {:?}",
        msg
    );
}

#[test]
fn success_data_passthrough_for_complex_type() {
    #[derive(serde::Serialize)]
    struct Inner {
        user_id: String,
        nick_name: String,
    }
    let resp = ApiResponse::ok(Inner {
        user_id: "u-1".into(),
        nick_name: "alice".into(),
    });
    let json = body_as_json(resp.into_response());
    assert_wire_shape(&json, false);
    let data = json["data"].as_object().unwrap();
    // The inner type's camelCase rules are its own concern — this test
    // just verifies the envelope passes through whatever the inner
    // Serialize emits.
    assert!(data.contains_key("user_id") || data.contains_key("userId"));
}
```

- [ ] **Step 7.2: Register the test module**

Edit `crates/framework/src/response/mod.rs`. Add at the bottom:

```rust
#[cfg(test)]
mod wire_test;
```

- [ ] **Step 7.3: Run the new test suite**

```bash
cd server-rs && cargo test -p framework wire_test 2>&1 | tail -20
```

Expected: 8 tests pass.

If any fail: the failure message will indicate exactly which wire shape rule is broken. Common failures and fixes:
- `wire uses msg, not message` — some `#[serde(rename = ...)]` typo crept in
- `camelCase requestId, not snake_case` — missing `#[serde(rename_all = "camelCase")]` on `ApiResponse`
- `unexpected top-level field` — someone added a field without coordinating
- `timestamp must be 24 chars` — format drift in `Utc::now().to_rfc3339_opts(...)`

- [ ] **Step 7.4: Add `http-body-util` dev-dependency if needed**

The `body_as_json` helper uses `http_body_util::BodyExt::collect`. Check `Cargo.toml`:

```bash
cd server-rs && grep -A 2 "\[dev-dependencies\]" crates/framework/Cargo.toml
```

If `http-body-util` isn't already a dev-dependency, add it:

```toml
[dev-dependencies]
# ... existing deps ...
http-body-util = "0.1"
```

Then re-run `cargo test -p framework wire_test`.

**Do not** add `http-body-util` to `[dependencies]` — it's only needed for this test. Adding to dev-deps is the minimal-impact move.

- [ ] **Step 7.5: Run full framework + workspace test**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo test --workspace 2>&1 | grep "test result"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected:
- framework: 78 passed (70 after Task 6 + 8 new wire_test tests)
- workspace: 161 passed (153 baseline + 6 from Tasks 2+6 framework tests — wait, recount)

Let me recount workspace totals across the plan:

| After Task | framework | workspace total |
|---|---|---|
| baseline | 69 | 153 |
| Task 1 (delete constructors) | 69 | 153 |
| Task 2 (delete Business fields) | 69 | 153 |
| Task 3 (merge envelope) | 69 | 153 |
| Task 4 (remove #[from]) | 69 | 153 |
| Task 5 (status_code test) | 69 | 153 (replaces old test) |
| Task 6 (i18n coverage) | 70 | 154 |
| Task 7 (wire regression) | 78 | 162 |

So after Task 7: **framework 78 / workspace 162**.

- [ ] **Step 7.6: Smoke test one more time**

```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null
sleep 1
cargo build -p app 2>&1 | tail -3
./target/debug/app > /tmp/tea-rs-err-v10-final.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
bash scripts/smoke-user-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null
wait 2>/dev/null
```

Expected: 14/14 + 16/16 PASSED.

- [ ] **Step 7.7: Report Task 7 complete**

8 wire regression tests in place, covering every `ApiResponse` / `AppError` response path. Framework test count at 78. Workspace at 162. Smoke green.

---

### Task 8: Update spec + commit prep

**Files:**
- Modify: `server-rs/docs/framework-error-envelope-spec.md`

Mark the §12 gap table items complete, update the status snapshot.

- [ ] **Step 8.1: Update §12 gap table**

Edit `server-rs/docs/framework-error-envelope-spec.md`. Find the §12 table. Add a new column "Status" and mark each row:

```markdown
| 规范条目 | 当前状态 | 需要改动 | Status |
|---|---|---|---|
| §2.1 单一 wire envelope | (was: 两份 struct) | (was: 合并) | ✅ v1.0 (Task 3) |
| §2.2 只保留 ok/success | (was: with_code 存在) | (was: 删除) | ✅ v1.0 (Task 1) |
| §2.3 删除 Business.params/data | (was: 死字段) | (was: 删除) | ✅ v1.0 (Task 2) |
| §2.3 删除 #[from] | (was: 隐式 From) | (was: 移除) | ✅ v1.0 (Task 4) |
| §2.3 删除 business_with_params | (was: 死方法) | (was: 删除) | ✅ v1.0 (Task 1) |
| §2.6 段位注册测试 | (was: 无) | (was: 新增) | ✅ v1.0 (Task 6) |
| §5.2 占位符 v1.0 现状 | (was: ACCOUNT_LOCKED latent) | (was: v1.1) | ⏳ v1.1 (deferred) |
| §6.1 field 路径 camelCase | (was: snake_case) | (was: v1.2) | ⏳ v1.2 (deferred) |
| §11 i18n 覆盖测试 | (was: 无) | (was: 新增) | ✅ v1.0 (Task 6) |
| wire 回归测试 | (was: 无) | (was: 新增) | ✅ v1.0 (Task 7) |
| 状态码映射单源 | (was: 散落 3 处) | (was: 合并) | ✅ v1.0 (Task 5) |
```

- [ ] **Step 8.2: Update §1 设计原则 reference**

Find 原则 5 in §1 that says "v0 两者都是 0 调用点死代码，v1.0 必须删除" — change tense from "必须删除" to "已删除（2026-04-11）" now that v1.0 is implemented.

- [ ] **Step 8.3: Update §12.1 code change summary**

Mark the 8 bullet points as complete, reference the task numbers.

- [ ] **Step 8.4: Run final verify**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo fmt --check && echo "fmt ok"
```

Expected:
- 162 tests passed (3 + 78 + 33 + 23 + 25)
- clippy clean
- fmt ok

- [ ] **Step 8.5: Report plan complete**

Report back with final summary:

- All 8 tasks complete
- Workspace tests: 153 → 162 (+9)
- Framework tests: 69 → 78 (+9)
- Dead code removed: `ApiResponse::with_code`, `AppError::business_with_params`, `AppError::Business.{params,data}`, `#[from] anyhow::Error`
- Single wire envelope: `ApiResponse<T>` is now the only response body struct
- CI regression tests: 8 wire shape tests + 1 i18n coverage test
- `ErrorBody` struct deleted
- Status code mapping has exhaustive-check test
- Spec §12 gap table updated to reflect completion
- Smoke tests 14/14 + 16/16 green
- Zero wire contract changes (byte-for-byte identical JSON output)
- Zero new runtime dependencies (http-body-util is dev-only)

Hand back to user for manual commit and merge.

---

## Post-plan status snapshot

After all 8 tasks:

| Metric | v0 baseline | v1.0 target |
|---|---|---|
| Total tests passing | 153 | 162 (+9) |
| Framework tests | 69 | 78 (+9) |
| Dead code methods | 3 | 0 |
| Dead code fields | 2 | 0 |
| Wire body structs | 2 (`ApiResponse<T>` + `ErrorBody`) | 1 (`ApiResponse<T>` only) |
| Status code map sources | 3 (fn + match + test) | 1 (match) + exhaustive-check test |
| i18n coverage enforcement | ❌ | ✅ (19-code explicit list) |
| Wire shape regression test | ❌ | ✅ (8 path coverage) |
| `#[from] anyhow::Error` implicit | ✅ (hazard) | ❌ (removed) |
| Wire contract changes | — | 0 |
| New runtime deps | — | 0 |
| New dev deps | — | `http-body-util` (for wire_test) |
| Smoke tests | 14/14 + 16/16 | 14/14 + 16/16 |

---

## What this plan explicitly doesn't do (deferred per spec §10)

1. **Error parameter substitution** — ACCOUNT_LOCKED's `{minutes}` placeholder stays as literal in v1.0. v1.1 adds `get_by_key_with_json_params` + a structured params API (coordinates with pagination v1.1 plan Task 7).
2. **FieldError.field camelCase unification** — currently uses snake_case Rust field names (`user_name`, `page.page_num`). v1.2 will coordinate with web/app client to flip to camelCase.
3. **Error `data` field carrying business payload** — spec §2.3 forbids it in v1.0. v2.0 revisits if a "partial success + details" case emerges.
4. **Cross-service correlation id** — v2.1 (triggered by second service or service mesh).
5. **OpenAPI / utoipa schema export** — v3.0 (triggered by introducing utoipa dep).
6. **Streaming / SSE / NDJSON primitive** — v3.1.
7. **Custom tower error layer** — rejected; `AppError::IntoResponse` is the single enforcement point.
8. **Error rate metrics / circuit breaker** — rejected as out of scope for error primitive (belongs in a separate observability spec).

---

## Execution handoff

Plan complete. Two execution options:

**1. Subagent-Driven (recommended)** — Controller dispatches fresh implementer + spec-reviewer + code-quality-reviewer subagents per task. Each task gets isolated context, two-stage review.

**2. Inline Execution** — Execute tasks in the current session with checkpoints between tasks for user review.

Which approach?
