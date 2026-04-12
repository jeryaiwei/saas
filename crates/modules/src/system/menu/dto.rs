//! Menu DTOs — wire shapes for `sys_menu` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::{MenuTreeRow, SysMenu};
use framework::response::fmt_ts;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Full menu detail. Returned by `GET /system/menu/{menuId}` and
/// `POST /system/menu/`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuDetailResponseDto {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
    pub order_num: i32,
    pub path: String,
    pub component: Option<String>,
    pub query: String,
    pub is_frame: String,
    pub is_cache: String,
    pub menu_type: String,
    pub visible: String,
    pub status: String,
    pub perms: String,
    pub icon: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
    pub del_flag: String,
    pub i18n: Option<serde_json::Value>,
}

impl MenuDetailResponseDto {
    pub fn from_entity(m: SysMenu) -> Self {
        Self {
            menu_id: m.menu_id,
            menu_name: m.menu_name,
            parent_id: m.parent_id,
            order_num: m.order_num,
            path: m.path,
            component: m.component,
            query: m.query,
            is_frame: m.is_frame,
            is_cache: m.is_cache,
            menu_type: m.menu_type,
            visible: m.visible,
            status: m.status,
            perms: m.perms,
            icon: m.icon,
            create_by: m.create_by,
            create_at: fmt_ts(&m.create_at),
            update_by: m.update_by,
            update_at: fmt_ts(&m.update_at),
            remark: m.remark,
            del_flag: m.del_flag,
            i18n: m.i18n,
        }
    }
}

/// Generic tree node — id, label, children. Used for dropdown tree UIs.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(no_recursion)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

/// Response for role-menu tree selection endpoints.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuTreeSelectResponseDto {
    pub menus: Vec<TreeNode>,
    pub checked_keys: Vec<String>,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Request body for `POST /system/menu/`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateMenuDto {
    #[validate(length(min = 1, max = 50))]
    pub menu_name: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub order_num: i32,
    #[validate(length(max = 200))]
    pub path: String,
    #[validate(length(max = 255))]
    pub component: Option<String>,
    #[validate(length(max = 255))]
    pub query: Option<String>,
    #[validate(length(min = 1, max = 1))]
    pub is_frame: String,
    #[validate(length(min = 1, max = 1))]
    pub is_cache: String,
    #[validate(length(min = 1, max = 1))]
    pub menu_type: String,
    #[validate(length(min = 1, max = 1))]
    pub visible: String,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 100))]
    pub perms: Option<String>,
    #[validate(length(max = 100))]
    pub icon: Option<String>,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

/// Request body for `PUT /system/menu/`.
/// `menu_id` is required; all other fields are optional.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMenuDto {
    pub menu_id: String,
    #[validate(length(min = 1, max = 50))]
    pub menu_name: Option<String>,
    pub parent_id: Option<String>,
    pub order_num: Option<i32>,
    #[validate(length(max = 200))]
    pub path: Option<String>,
    #[validate(length(max = 255))]
    pub component: Option<String>,
    #[validate(length(max = 255))]
    pub query: Option<String>,
    #[validate(length(min = 1, max = 1))]
    pub is_frame: Option<String>,
    #[validate(length(min = 1, max = 1))]
    pub is_cache: Option<String>,
    #[validate(length(min = 1, max = 1))]
    pub menu_type: Option<String>,
    #[validate(length(min = 1, max = 1))]
    pub visible: Option<String>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    #[validate(length(max = 100))]
    pub perms: Option<String>,
    #[validate(length(max = 100))]
    pub icon: Option<String>,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

/// Query string for `GET /system/menu/list`. Non-paginated.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMenuDto {
    pub menu_name: Option<String>,
    pub status: Option<String>,
    pub parent_id: Option<String>,
    pub menu_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Tree builder
// ---------------------------------------------------------------------------

/// Build a tree from flat `MenuTreeRow` rows. O(n) using index maps.
pub fn list_to_tree(rows: Vec<MenuTreeRow>) -> Vec<TreeNode> {
    use std::collections::HashMap;

    if rows.is_empty() {
        return vec![];
    }

    // Map menu_id -> index into `nodes`.
    let mut id_to_idx: HashMap<String, usize> = HashMap::with_capacity(rows.len());
    // Collect children indices: parent_idx -> [child_idx, ...]
    let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();

    // Build flat node list and id→index map.
    let mut nodes: Vec<TreeNode> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            id_to_idx.insert(row.menu_id.clone(), i);
            TreeNode {
                id: row.menu_id.clone(),
                label: row.menu_name.clone(),
                children: vec![],
            }
        })
        .collect();

    // Identify roots and build children_map.
    let mut root_indices: Vec<usize> = vec![];
    for (i, row) in rows.iter().enumerate() {
        match &row.parent_id {
            Some(pid) if id_to_idx.contains_key(pid.as_str()) => {
                let parent_idx = id_to_idx[pid.as_str()];
                children_map.entry(parent_idx).or_default().push(i);
            }
            _ => root_indices.push(i),
        }
    }

    // Recursively attach children (bottom-up via stack to avoid deep recursion issues).
    fn attach_children(
        idx: usize,
        nodes: &mut Vec<TreeNode>,
        children_map: &HashMap<usize, Vec<usize>>,
    ) -> TreeNode {
        let child_indices = children_map.get(&idx).cloned().unwrap_or_default();
        let children: Vec<TreeNode> = child_indices
            .into_iter()
            .map(|ci| attach_children(ci, nodes, children_map))
            .collect();
        TreeNode {
            id: nodes[idx].id.clone(),
            label: nodes[idx].label.clone(),
            children,
        }
    }

    root_indices
        .into_iter()
        .map(|ri| attach_children(ri, &mut nodes, &children_map))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MenuTreeRow;

    #[test]
    fn list_to_tree_empty() {
        let result = list_to_tree(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn list_to_tree_flat_roots() {
        let rows = vec![
            MenuTreeRow {
                menu_id: "1".into(),
                menu_name: "A".into(),
                parent_id: None,
            },
            MenuTreeRow {
                menu_id: "2".into(),
                menu_name: "B".into(),
                parent_id: None,
            },
        ];
        let tree = list_to_tree(rows);
        assert_eq!(tree.len(), 2);
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn list_to_tree_nested() {
        let rows = vec![
            MenuTreeRow {
                menu_id: "1".into(),
                menu_name: "Root".into(),
                parent_id: None,
            },
            MenuTreeRow {
                menu_id: "2".into(),
                menu_name: "Child".into(),
                parent_id: Some("1".into()),
            },
            MenuTreeRow {
                menu_id: "3".into(),
                menu_name: "GrandChild".into(),
                parent_id: Some("2".into()),
            },
        ];
        let tree = list_to_tree(rows);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].children.len(), 1);
    }
}
