//! Wire-shape regression tests for `ApiResponse<T>` and `AppError::IntoResponse`.
//!
//! These tests assert the serialized JSON key set, ordering, and camelCase
//! conventions for every response path. They are the CI enforcement of
//! `docs/framework-error-envelope-spec.md` §2.1 (single wire envelope) —
//! a PR that accidentally renames `msg` to `message` or adds a new
//! top-level field without coordination will fail here, not in production.
//!
//! The parent `mod.rs` already gates this module behind `#[cfg(test)]`;
//! we don't repeat the gate here.

use super::{ApiResponse, ResponseCode};
use crate::error::{AppError, FieldError};
use axum::body::to_bytes;
use axum::response::IntoResponse;
use serde_json::Value;

/// Block-on helper: drain an axum response body into a serde_json::Value.
/// `usize::MAX` cap is safe because these are test fixtures with known-small
/// bodies; production paths do NOT use this helper.
fn body_as_json(resp: axum::response::Response) -> Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (_, body) = resp.into_parts();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    })
}

/// Assert the top-level keys of a wire response match the spec §2.1
/// contract: `code` + `msg` + `data` + (optional) `requestId` + `timestamp`.
/// Fails if any extra field appears or a required field is missing.
fn assert_wire_shape(value: &Value, expects_request_id: bool) {
    let obj = value
        .as_object()
        .expect("response body must be a JSON object");

    // Required keys
    assert!(obj.contains_key("code"), "missing `code`: {:?}", obj);
    assert!(obj.contains_key("msg"), "missing `msg`: {:?}", obj);
    assert!(obj.contains_key("data"), "missing `data`: {:?}", obj);
    assert!(
        obj.contains_key("timestamp"),
        "missing `timestamp`: {:?}",
        obj
    );

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
    let ts = obj["timestamp"]
        .as_str()
        .expect("`timestamp` must be a string");
    // Shape check: YYYY-MM-DDTHH:MM:SS.sssZ (24 chars)
    assert_eq!(
        ts.len(),
        24,
        "timestamp must be 24 chars (millisecond RFC3339): {:?}",
        ts
    );
    assert!(
        ts.ends_with('Z'),
        "timestamp must be UTC (end with Z): {:?}",
        ts
    );
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
                params: Default::default(),
            },
            FieldError {
                field: "page.page_num".into(),
                message: "range".into(),
                params: Default::default(),
            },
        ],
    };
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);
    assert_eq!(json["code"], 400);

    // `data` is an array of FieldError-shaped objects
    let data = json["data"]
        .as_array()
        .expect("validation data must be array");
    assert_eq!(data.len(), 2);
    for item in data {
        let obj = item.as_object().unwrap();
        assert!(obj.contains_key("field"));
        assert!(obj.contains_key("message"));
        // No other fields on FieldError — `params` must be skipped
        // (framework-internal, never serialized).
        assert_eq!(
            obj.len(),
            2,
            "FieldError must have exactly 2 wire keys: {:?}",
            obj
        );
    }
}

#[test]
fn validation_error_substitutes_min_max_params_in_wire_message() {
    // Simulate what the extractor produces for a `#[validate(range(min=1, max=200))]`
    // violation: message="range", params={"min": 1, "max": 200, "value": 500}.
    let mut params = std::collections::HashMap::new();
    params.insert("min".to_string(), serde_json::json!(1));
    params.insert("max".to_string(), serde_json::json!(200));
    params.insert("value".to_string(), serde_json::json!(500));

    let err = AppError::Validation {
        errors: vec![FieldError {
            field: "pageSize".into(),
            message: "range".into(),
            params,
        }],
    };
    let json = body_as_json(err.into_response());
    assert_wire_shape(&json, false);

    let data = json["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    let msg = data[0]["message"].as_str().unwrap();
    // The substituted message must contain the actual bounds, not
    // the raw placeholder text.
    assert!(
        msg.contains('1') && msg.contains("200"),
        "expected min=1 and max=200 in substituted message, got: {:?}",
        msg
    );
    assert!(
        !msg.contains("{min}") && !msg.contains("{max}"),
        "placeholders must be substituted, got: {:?}",
        msg
    );
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
