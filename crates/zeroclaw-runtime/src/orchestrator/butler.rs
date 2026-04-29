use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButlerRuntimeHandle {
    pub user_id: String,
    pub butler_id: String,
    pub host_id: String,
}

impl ButlerRuntimeHandle {
    pub fn new(
        user_id: impl Into<String>,
        butler_id: impl Into<String>,
        host_id: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            butler_id: butler_id.into(),
            host_id: host_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn butler_runtime_handle_keeps_user_and_host_binding() {
        let handle = ButlerRuntimeHandle::new("user-a", "butler-1", "host-a");

        assert_eq!(handle.user_id, "user-a");
        assert_eq!(handle.butler_id, "butler-1");
        assert_eq!(handle.host_id, "host-a");
    }
}
