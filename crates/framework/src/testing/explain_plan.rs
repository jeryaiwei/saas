//! Postgres `EXPLAIN (FORMAT JSON)` plan assertion helpers.
//!
//! These functions take `&serde_json::Value` (the decoded plan JSON) and
//! walk the node tree to check specific properties. They are intentionally
//! pure — no sqlx dependency, no async — so they can be unit-tested with
//! fixture JSON and reused from any crate.

use serde_json::Value;

/// Walk a Postgres `EXPLAIN (FORMAT JSON)` plan tree and return `Err`
/// if any node's `"Node Type"` is `"Seq Scan"` on a table not in the
/// `exempt_tables` allowlist. Small tables (like dictionaries, enum
/// lookups, or single-tenant config rows) typically want to be exempt
/// because seq scan is actually the optimal plan for <1k-row tables.
///
/// The `plan_root` argument is the raw JSON returned by
/// `EXPLAIN (FORMAT JSON) ...`, which Postgres wraps in an outer array
/// with a single `{"Plan": {...}}` element.
///
/// # Example
///
/// ```ignore
/// let plan_str: String = sqlx::query_scalar(
///     "EXPLAIN (FORMAT JSON) SELECT ... FROM sys_user u WHERE ...",
/// )
/// .bind(...)
/// .fetch_one(&pool)
/// .await?;
/// let plan: serde_json::Value = serde_json::from_str(&plan_str)?;
/// check_no_seq_scan(&plan, &["sys_dict", "sys_config"])?;
/// ```
pub fn check_no_seq_scan(plan_root: &Value, exempt_tables: &[&str]) -> Result<(), String> {
    let root_node = plan_root
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|obj| obj.get("Plan"))
        .ok_or_else(|| {
            "unexpected EXPLAIN (FORMAT JSON) output shape — \
             expected outer array with .Plan element"
                .to_string()
        })?;
    walk_plan_node(root_node, exempt_tables)
}

fn walk_plan_node(node: &Value, exempt: &[&str]) -> Result<(), String> {
    let node_type = node
        .get("Node Type")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>");

    if node_type == "Seq Scan" {
        let relation = node
            .get("Relation Name")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        if !exempt.contains(&relation) {
            return Err(format!(
                "seq scan detected on non-exempt table: {} \
                 (exempt list: {:?})",
                relation, exempt
            ));
        }
    }

    if let Some(subplans) = node.get("Plans").and_then(|v| v.as_array()) {
        for sub in subplans {
            walk_plan_node(sub, exempt)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_index_scan_passes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Index Scan",
                "Relation Name": "sys_user",
                "Plans": []
            }
        }]);
        assert!(check_no_seq_scan(&plan, &[]).is_ok());
    }

    #[test]
    fn flat_seq_scan_on_non_exempt_fails() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "sys_user"
            }
        }]);
        let err = check_no_seq_scan(&plan, &[]).unwrap_err();
        assert!(err.contains("sys_user"));
        assert!(err.contains("seq scan"));
    }

    #[test]
    fn seq_scan_on_exempt_table_passes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "sys_dict"
            }
        }]);
        assert!(check_no_seq_scan(&plan, &["sys_dict"]).is_ok());
    }

    #[test]
    fn nested_seq_scan_inside_hash_join_fails() {
        // Typical JOIN plan: outer Hash Join with inner Seq Scan on a
        // non-exempt table. The recursive walk must catch this.
        let plan = json!([{
            "Plan": {
                "Node Type": "Hash Join",
                "Plans": [
                    {
                        "Node Type": "Index Scan",
                        "Relation Name": "sys_user_tenant"
                    },
                    {
                        "Node Type": "Hash",
                        "Plans": [
                            {
                                "Node Type": "Seq Scan",
                                "Relation Name": "sys_user"
                            }
                        ]
                    }
                ]
            }
        }]);
        let err = check_no_seq_scan(&plan, &[]).unwrap_err();
        assert!(err.contains("sys_user"));
    }

    #[test]
    fn nested_seq_scan_on_exempt_table_passes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Nested Loop",
                "Plans": [
                    {
                        "Node Type": "Seq Scan",
                        "Relation Name": "sys_dict"
                    },
                    {
                        "Node Type": "Index Scan",
                        "Relation Name": "sys_user"
                    }
                ]
            }
        }]);
        assert!(check_no_seq_scan(&plan, &["sys_dict"]).is_ok());
    }

    #[test]
    fn unexpected_shape_returns_descriptive_error() {
        let plan = json!({"not": "an array"});
        let err = check_no_seq_scan(&plan, &[]).unwrap_err();
        assert!(err.contains("unexpected EXPLAIN"));
    }
}
