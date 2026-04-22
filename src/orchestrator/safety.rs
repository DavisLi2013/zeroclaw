#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSafetyDecision {
    Allow,
    Reject,
    Sandbox,
}

#[derive(Debug, Clone, Default)]
pub struct ToolSafetyReviewGate;

impl ToolSafetyReviewGate {
    pub fn review(&self, tool_name: &str, visible_tools: &[String]) -> ToolSafetyDecision {
        if !visible_tools.iter().any(|visible| visible == tool_name) {
            return ToolSafetyDecision::Reject;
        }

        if matches!(tool_name, "browser_delegate" | "web_fetch" | "browser") {
            ToolSafetyDecision::Sandbox
        } else {
            ToolSafetyDecision::Allow
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundAuditDecision {
    pub allowed: bool,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OutboundSafetyAuditGate;

impl OutboundSafetyAuditGate {
    pub fn review(
        &self,
        response_payload: serde_json::Value,
        safety_labels: &[String],
    ) -> OutboundAuditDecision {
        let mut labels = Vec::new();

        if safety_labels
            .iter()
            .any(|label| label.starts_with("user_private_"))
        {
            labels.push("private-memory-redaction".to_string());
        }

        if response_payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| text.contains("sk-"))
        {
            labels.push("credential-leak-redaction".to_string());
        }

        OutboundAuditDecision {
            allowed: true,
            labels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_safety_gate_rejects_tool_not_present_in_tool_surface() {
        let decision = ToolSafetyReviewGate::default()
            .review("shell", &["read_file".to_string(), "web_fetch".to_string()]);

        assert_eq!(decision, ToolSafetyDecision::Reject);
    }

    #[test]
    fn tool_safety_gate_sandboxes_browser_style_tools() {
        let decision = ToolSafetyReviewGate::default()
            .review("browser_delegate", &["browser_delegate".to_string()]);

        assert_eq!(decision, ToolSafetyDecision::Sandbox);
    }

    #[test]
    fn outbound_safety_gate_flags_private_memory_labels() {
        let decision = OutboundSafetyAuditGate::default().review(
            serde_json::json!({"text": "secret"}),
            &["user_private_long_term".to_string()],
        );

        assert!(decision.allowed);
        assert_eq!(decision.labels, vec!["private-memory-redaction".to_string()]);
    }
}
