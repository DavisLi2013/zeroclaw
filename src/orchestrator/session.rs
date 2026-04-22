use serde::{Deserialize, Serialize};

use crate::orchestrator::contracts::ContextBuildReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Running,
    WaitingTool,
    WaitingAgent,
    Cancelling,
    Failed,
    Rebuilding,
    Closed,
}

impl SessionPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::WaitingTool => "waiting_tool",
            Self::WaitingAgent => "waiting_agent",
            Self::Cancelling => "cancelling",
            Self::Failed => "failed",
            Self::Rebuilding => "rebuilding",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeBundle {
    pub session_id: String,
    pub turn_queue_depth: usize,
    pub active_turn_id: Option<String>,
    pub capability_snapshot_version: String,
    pub tool_surface_version: String,
    pub skill_catalog_version: String,
    pub memory_snapshot_version: String,
    pub context_build_reason: ContextBuildReason,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_runtime_bundle_captures_frozen_snapshots() {
        let bundle = SessionRuntimeBundle {
            session_id: "session-a".into(),
            turn_queue_depth: 0,
            active_turn_id: None,
            capability_snapshot_version: "cap-v1".into(),
            tool_surface_version: "tool-v1".into(),
            skill_catalog_version: "skill-v1".into(),
            memory_snapshot_version: "mem-v1".into(),
            context_build_reason: ContextBuildReason::SessionBootstrap,
        };

        assert_eq!(bundle.session_id, "session-a");
        assert_eq!(
            bundle.context_build_reason,
            ContextBuildReason::SessionBootstrap
        );
    }

    #[test]
    fn session_phase_contract_tracks_cancelling_state() {
        assert_eq!(SessionPhase::Cancelling.as_str(), "cancelling");
    }
}
