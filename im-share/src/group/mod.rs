use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;

// Simple in-memory mock group directory shared by server/connect for demo
static GROUP_MEMBERS: Lazy<RwLock<HashMap<String, Vec<String>>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "g1".to_string(),
        vec!["u1".to_string(), "u2".to_string(), "u3".to_string()],
    );
    map.insert("g2".to_string(), vec!["u2".to_string(), "u4".to_string()]);
    RwLock::new(map)
});

pub fn get_group_members(group_id: &str) -> Vec<String> {
    GROUP_MEMBERS
        .read()
        .ok()
        .and_then(|m| m.get(group_id).cloned())
        .unwrap_or_default()
}

pub fn set_group_members(group_id: &str, members: Vec<String>) {
    if let Ok(mut m) = GROUP_MEMBERS.write() {
        m.insert(group_id.to_string(), members);
    }
}

