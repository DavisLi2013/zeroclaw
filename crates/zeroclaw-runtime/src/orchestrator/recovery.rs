use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryCursor {
    pub user_id: String,
    pub session_id: String,
    pub descriptor_version_set: Vec<String>,
    pub last_turn_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_cursor_captures_session_runtime_versions() {
        let cursor = RecoveryCursor {
            user_id: "user-a".into(),
            session_id: "session-a".into(),
            descriptor_version_set: vec!["cap-a@1.0.0".into()],
            last_turn_id: Some("turn-9".into()),
        };

        assert_eq!(cursor.session_id, "session-a");
        assert_eq!(cursor.descriptor_version_set, vec!["cap-a@1.0.0"]);
    }
}
