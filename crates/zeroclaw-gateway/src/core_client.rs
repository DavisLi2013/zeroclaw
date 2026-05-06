use std::pin::Pin;

use futures_util::Stream;

pub type CoreRunStream = Pin<Box<dyn Stream<Item = anyhow::Result<CoreRunEvent>> + Send + 'static>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreRunRequest {
    pub request_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub input: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreRunEvent {
    RunStarted {
        provider: String,
        model: String,
    },
    MessageDelta {
        delta: String,
    },
    ThinkingDelta {
        delta: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        id: String,
        name: String,
        output: String,
    },
    Completed {
        final_text: String,
    },
    Cancelled {
        reason: String,
    },
    Failed {
        code: String,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCancelResult {
    pub accepted: bool,
}

#[derive(Default)]
pub struct CoreRunCollected {
    pub final_text: String,
}

impl CoreRunCollected {
    pub fn apply(&mut self, event: &CoreRunEvent) {
        if let CoreRunEvent::MessageDelta { delta } = event {
            self.final_text.push_str(delta);
        }
        if let CoreRunEvent::Completed { final_text } = event {
            self.final_text = final_text.clone();
        }
    }
}

#[async_trait::async_trait]
pub trait CoreAgentClient: Send + Sync {
    async fn run_chat_streamed(&self, request: CoreRunRequest) -> anyhow::Result<CoreRunStream>;

    async fn cancel_run(&self, run_id: &str, reason: &str) -> anyhow::Result<CoreCancelResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_delta_accumulates_final_text() {
        let mut collected = CoreRunCollected::default();
        collected.apply(&CoreRunEvent::MessageDelta {
            delta: "hello".to_string(),
        });
        collected.apply(&CoreRunEvent::MessageDelta {
            delta: " world".to_string(),
        });

        assert_eq!(collected.final_text, "hello world");
    }
}
