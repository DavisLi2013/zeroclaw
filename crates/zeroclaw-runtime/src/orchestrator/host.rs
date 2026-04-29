use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserHost {
    pub host_id: String,
    pub user_id: String,
    pub butler_id: Option<String>,
    pub session_ids: Vec<String>,
}

impl UserHost {
    pub fn new(host_id: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            host_id: host_id.into(),
            user_id: user_id.into(),
            butler_id: None,
            session_ids: Vec::new(),
        }
    }

    pub fn with_butler(mut self, butler_id: impl Into<String>) -> Self {
        self.butler_id = Some(butler_id.into());
        self
    }

    pub fn with_sessions(mut self, session_ids: Vec<String>) -> Self {
        self.session_ids = session_ids;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_host_tracks_butler_and_session_ids() {
        let host = UserHost::new("host-a", "user-a")
            .with_butler("butler-1")
            .with_sessions(vec!["session-1".to_string(), "session-2".to_string()]);

        assert_eq!(host.host_id, "host-a");
        assert_eq!(host.user_id, "user-a");
        assert_eq!(host.butler_id.as_deref(), Some("butler-1"));
        assert_eq!(host.session_ids.len(), 2);
    }
}
