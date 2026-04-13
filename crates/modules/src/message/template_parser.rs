//! Template parser — shared `${key}` substitution for mail/SMS templates.
//!
//! Uses simple string scanning (no regex) to find `${key}` placeholders,
//! extract parameter names, validate presence, and render substitutions.

use std::collections::HashMap;

use framework::error::AppError;
use framework::response::ResponseCode;

/// Extract all `${key}` parameter names from a template string.
///
/// Scans for `"${"` followed by `"}"` and collects the key name between them.
/// Malformed tokens (e.g. unclosed `${`) are silently skipped.
/// Duplicate keys are included only once, in first-occurrence order.
pub fn extract_params(template: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for "${"
        if let Some(pos) = template[i..].find("${") {
            let start = i + pos + 2; // index right after "${"
            if start >= len {
                break;
            }
            // Look for closing "}"
            if let Some(end_offset) = template[start..].find('}') {
                let key = &template[start..start + end_offset];
                if !key.is_empty() && !seen.contains(key) {
                    seen.insert(key.to_string());
                    result.push(key.to_string());
                }
                i = start + end_offset + 1; // move past the "}"
            } else {
                // Unclosed "${" — skip past it and stop (no more "}" in rest)
                break;
            }
        } else {
            // No more "${" found
            break;
        }
    }

    result
}

/// Validate that `params` covers all required keys in the template.
///
/// Returns `Ok(())` if every `${key}` in the template has a corresponding
/// entry in `params`. Otherwise returns a `BusinessWithMsg` error with the
/// given `code` and a message listing the missing parameter names.
pub fn validate_params(
    template: &str,
    params: &HashMap<String, String>,
    code: ResponseCode,
) -> Result<(), AppError> {
    let required = extract_params(template);
    let missing: Vec<&str> = required
        .iter()
        .filter(|k| !params.contains_key(k.as_str()))
        .map(|k| k.as_str())
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(AppError::business_with_msg(
            code,
            format!("Missing template params: {}", missing.join(", ")),
        ))
    }
}

/// Render: replace all `${key}` occurrences with values from `params`.
///
/// Assumes `validate_params` was called first. Unknown keys (not present
/// in `params`) are left as-is in the output.
pub fn render(template: &str, params: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(template.len());
    let len = template.len();
    let mut i = 0;

    while i < len {
        if let Some(pos) = template[i..].find("${") {
            // Append everything before "${"
            result.push_str(&template[i..i + pos]);
            let start = i + pos + 2; // index right after "${"
            if start >= len {
                // Unclosed at end — append the literal "${" and stop
                result.push_str("${");
                break;
            }
            if let Some(end_offset) = template[start..].find('}') {
                let key = &template[start..start + end_offset];
                if let Some(value) = params.get(key) {
                    result.push_str(value);
                } else {
                    // Unknown key — leave as-is
                    result.push_str("${");
                    result.push_str(key);
                    result.push('}');
                }
                i = start + end_offset + 1;
            } else {
                // Unclosed "${" — append literal and consume rest
                result.push_str(&template[i + pos..]);
                break;
            }
        } else {
            // No more "${" — append remainder
            result.push_str(&template[i..]);
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_params ───────────────────────────────────────────────

    #[test]
    fn extract_params_normal() {
        let tpl = "Hello ${name}, your code is ${code}.";
        let params = extract_params(tpl);
        assert_eq!(params, vec!["name", "code"]);
    }

    #[test]
    fn extract_params_empty_template() {
        assert!(extract_params("").is_empty());
    }

    #[test]
    fn extract_params_no_params() {
        assert!(extract_params("Hello world, no placeholders here.").is_empty());
    }

    #[test]
    fn extract_params_malformed_unclosed() {
        // Unclosed ${ should be skipped
        let tpl = "Hello ${name, goodbye";
        assert!(extract_params(tpl).is_empty());
    }

    #[test]
    fn extract_params_nested_dollar_brace() {
        // "${a${b}}" — scans linearly: first "${" finds key "a${b"? No.
        // Actually: first "${"  at index 0 → looks for "}" → finds at
        // index where "}" is. Let's trace: "a${b}" — the "}" at pos 5
        // gives key "a${b". That's treated as the key name.
        let tpl = "${a${b}}";
        let params = extract_params(tpl);
        // First "${" at 0, key search finds first "}" at index 5 (after "b"),
        // so key = "a${b". Then continues from index 6 which is "}".
        assert_eq!(params, vec!["a${b"]);
    }

    #[test]
    fn extract_params_duplicates_deduplicated() {
        let tpl = "${x} and ${x} and ${y}";
        let params = extract_params(tpl);
        assert_eq!(params, vec!["x", "y"]);
    }

    #[test]
    fn extract_params_empty_key_skipped() {
        // "${}" has an empty key — should be skipped
        let tpl = "before ${} after ${ok}";
        let params = extract_params(tpl);
        assert_eq!(params, vec!["ok"]);
    }

    // ── render ───────────────────────────────────────────────────────

    #[test]
    fn render_normal_substitution() {
        let tpl = "Hello ${name}, your code is ${code}.";
        let mut params = HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        params.insert("code".to_string(), "1234".to_string());
        assert_eq!(render(tpl, &params), "Hello Alice, your code is 1234.");
    }

    #[test]
    fn render_empty_params_leaves_placeholders() {
        let tpl = "Hello ${name}!";
        let params = HashMap::new();
        assert_eq!(render(tpl, &params), "Hello ${name}!");
    }

    #[test]
    fn render_partial_params() {
        let tpl = "${a} and ${b}";
        let mut params = HashMap::new();
        params.insert("a".to_string(), "X".to_string());
        assert_eq!(render(tpl, &params), "X and ${b}");
    }

    #[test]
    fn render_no_placeholders() {
        let tpl = "No placeholders here.";
        let params = HashMap::new();
        assert_eq!(render(tpl, &params), "No placeholders here.");
    }

    #[test]
    fn render_unclosed_placeholder() {
        let tpl = "Hello ${name";
        let params = HashMap::new();
        assert_eq!(render(tpl, &params), "Hello ${name");
    }

    // ── validate_params ──────────────────────────────────────────────

    #[test]
    fn validate_params_all_present() {
        let tpl = "Hello ${name}, code ${code}";
        let mut params = HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        params.insert("code".to_string(), "123".to_string());
        assert!(validate_params(tpl, &params, ResponseCode::MAIL_TEMPLATE_PARAMS_MISSING).is_ok());
    }

    #[test]
    fn validate_params_missing_keys() {
        let tpl = "Hello ${name}, code ${code}, extra ${extra}";
        let mut params = HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        let err = validate_params(tpl, &params, ResponseCode::SMS_TEMPLATE_PARAMS_MISSING)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("code"), "should mention missing key 'code': {msg}");
        assert!(msg.contains("extra"), "should mention missing key 'extra': {msg}");
    }

    #[test]
    fn validate_params_no_placeholders() {
        let tpl = "Plain text with no params";
        let params = HashMap::new();
        assert!(validate_params(tpl, &params, ResponseCode::MAIL_TEMPLATE_PARAMS_MISSING).is_ok());
    }
}
