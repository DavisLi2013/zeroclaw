use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnqueuePolicy {
    Enqueue,
    InterruptAndEnqueue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLoadingDirectives {
    pub public_short_term: bool,
    pub public_long_term: bool,
    pub user_private_short_term: bool,
    pub user_private_long_term: bool,
}

impl Default for MemoryLoadingDirectives {
    fn default() -> Self {
        Self::all_enabled()
    }
}

impl MemoryLoadingDirectives {
    pub fn all_enabled() -> Self {
        Self {
            public_short_term: true,
            public_long_term: true,
            user_private_short_term: true,
            user_private_long_term: true,
        }
    }

    pub fn public_only() -> Self {
        Self {
            public_short_term: true,
            public_long_term: true,
            user_private_short_term: false,
            user_private_long_term: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundMessage {
    pub message_id: String,
    pub channel_type: String,
    pub session_hint: Option<String>,
    pub enqueue_policy: EnqueuePolicy,
    pub auth_material_ref: Option<String>,
    pub request_payload: serde_json::Value,
    pub memory_loading_directives: MemoryLoadingDirectives,
    pub trace_context: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub message_id: String,
    pub correlation_id: String,
    pub user_id: String,
    pub session_id: String,
    pub channel_type: String,
    pub response_payload: serde_json::Value,
    pub safety_labels: Vec<String>,
    pub audit_result: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBuildReason {
    SessionBootstrap,
    TurnGeneration,
    SkillBodyReentry,
    ToolResultReentry,
    AgentResultReentry,
    MemoryAppendReentry,
    RetryRebuild,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_message_serializes_with_enqueue_policy_and_memory_directives() {
        let message = InboundMessage {
            message_id: "msg-1".into(),
            channel_type: "http".into(),
            session_hint: Some("session-a".into()),
            enqueue_policy: EnqueuePolicy::InterruptAndEnqueue,
            auth_material_ref: Some("auth/ref".into()),
            request_payload: serde_json::json!({"input": "hello"}),
            memory_loading_directives: MemoryLoadingDirectives::all_enabled(),
            trace_context: serde_json::json!({"trace_id": "trace-1"}),
            created_at: "2026-04-22T00:00:00Z".into(),
        };

        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["enqueue_policy"], "interrupt_and_enqueue");
        assert_eq!(
            json["memory_loading_directives"]["user_private_long_term"],
            true
        );
    }

    #[test]
    fn public_only_memory_directives_disable_private_scopes() {
        let directives = MemoryLoadingDirectives::public_only();
        assert!(directives.public_short_term);
        assert!(directives.public_long_term);
        assert!(!directives.user_private_short_term);
        assert!(!directives.user_private_long_term);
    }

    #[test]
    fn context_build_reason_uses_snake_case_contract_names() {
        let json = serde_json::to_string(&ContextBuildReason::SkillBodyReentry).unwrap();
        assert_eq!(json, "\"skill_body_reentry\"");
    }
}
