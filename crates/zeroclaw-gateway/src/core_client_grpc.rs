use std::time::Duration;

use crate::core_client::{
    CoreAgentClient, CoreCancelResult, CoreRunEvent, CoreRunRequest, CoreRunStream,
};
use crate::grpc::pb;
use futures_util::StreamExt;
use http::uri::PathAndQuery;
use tonic::Request;
use tonic::client::Grpc;
use tonic::transport::{Channel, Endpoint};
use tonic_prost::ProstCodec;

const CREATE_RUN_PATH: &str = "/zeroclaw.v1.AgentService/CreateRun";
const STREAM_RUN_PATH: &str = "/zeroclaw.v1.AgentService/StreamRun";
const CANCEL_RUN_PATH: &str = "/zeroclaw.v1.AgentService/CancelRun";

pub struct GrpcCoreAgentClient {
    endpoint: String,
    bearer_token: Option<String>,
    timeout: Duration,
}

impl GrpcCoreAgentClient {
    pub fn new(
        endpoint: String,
        bearer_token: Option<String>,
        timeout: Duration,
    ) -> anyhow::Result<Self> {
        if endpoint.trim().is_empty() {
            anyhow::bail!("core gRPC endpoint must not be empty");
        }
        Ok(Self {
            endpoint,
            bearer_token,
            timeout,
        })
    }

    async fn connect(&self) -> anyhow::Result<Grpc<Channel>> {
        let channel = Endpoint::from_shared(self.endpoint.clone())?
            .timeout(self.timeout)
            .connect()
            .await?;
        Ok(Grpc::new(channel))
    }

    fn request<T>(&self, message: T) -> anyhow::Result<Request<T>> {
        let mut request = Request::new(message);
        if let Some(token) = self
            .bearer_token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            let value = format!("Bearer {token}").parse()?;
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

#[async_trait::async_trait]
impl CoreAgentClient for GrpcCoreAgentClient {
    async fn run_chat_streamed(&self, request: CoreRunRequest) -> anyhow::Result<CoreRunStream> {
        let mut client = self.connect().await?;
        let create = self.request(build_create_run_request(request))?;
        let create_response: pb::CreateRunResponse = client
            .unary(
                create,
                PathAndQuery::from_static(CREATE_RUN_PATH),
                ProstCodec::default(),
            )
            .await?
            .into_inner();

        let stream_request = self.request(pb::StreamRunRequest {
            run_id: create_response.run_id,
            after_sequence: 0,
        })?;
        let stream = client
            .server_streaming(
                stream_request,
                PathAndQuery::from_static(STREAM_RUN_PATH),
                ProstCodec::default(),
            )
            .await?
            .into_inner();

        Ok(Box::pin(stream.map(|event| match event {
            Ok(event) => map_grpc_event(event),
            Err(status) => Err(anyhow::anyhow!(status)),
        })))
    }

    async fn cancel_run(&self, run_id: &str, reason: &str) -> anyhow::Result<CoreCancelResult> {
        let mut client = self.connect().await?;
        let response: pb::CancelRunResponse = client
            .unary(
                self.request(pb::CancelRunRequest {
                    run_id: run_id.to_string(),
                    reason: reason.to_string(),
                })?,
                PathAndQuery::from_static(CANCEL_RUN_PATH),
                ProstCodec::default(),
            )
            .await?
            .into_inner();

        Ok(CoreCancelResult {
            accepted: response.accepted,
        })
    }
}

pub fn build_create_run_request(request: CoreRunRequest) -> pb::CreateRunRequest {
    pb::CreateRunRequest {
        protocol: "zeroclaw.v1".to_string(),
        request_id: request.request_id,
        session_id: request.session_id,
        actor: Some(pb::Actor {
            actor_id: request.actor_id,
            actor_type: "edge-user".to_string(),
            display_name: String::new(),
            metadata: Default::default(),
        }),
        input: Some(pb::RunInput {
            kind: pb::run_input::InputKind::Message as i32,
            text: request.input,
        }),
        options: Some(pb::RunOptions {
            stream: true,
            model: String::new(),
            allowed_tools: Vec::new(),
            timeout_ms: 0,
        }),
        metadata: Default::default(),
    }
}

pub fn map_grpc_event(event: pb::RunEvent) -> anyhow::Result<CoreRunEvent> {
    match event.payload {
        Some(pb::run_event::Payload::Started(started)) => Ok(CoreRunEvent::RunStarted {
            provider: started.provider,
            model: started.model,
        }),
        Some(pb::run_event::Payload::MessageDelta(delta)) => {
            Ok(CoreRunEvent::MessageDelta { delta: delta.delta })
        }
        Some(pb::run_event::Payload::ThinkingDelta(delta)) => {
            Ok(CoreRunEvent::ThinkingDelta { delta: delta.delta })
        }
        Some(pb::run_event::Payload::ToolCall(call)) => Ok(CoreRunEvent::ToolCall {
            id: call.id,
            name: call.name,
            args: serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::Value::Null),
        }),
        Some(pb::run_event::Payload::ToolResult(result)) => Ok(CoreRunEvent::ToolResult {
            id: result.id,
            name: result.name,
            output: result.output,
        }),
        Some(pb::run_event::Payload::Completed(done)) => Ok(CoreRunEvent::Completed {
            final_text: done.final_text,
        }),
        Some(pb::run_event::Payload::Cancelled(cancelled)) => Ok(CoreRunEvent::Cancelled {
            reason: cancelled.reason,
        }),
        Some(pb::run_event::Payload::Failed(failed)) => {
            let error = failed.error.unwrap_or(pb::RunError {
                code: "unknown".to_string(),
                message: "run failed".to_string(),
                retryable: false,
                details: Default::default(),
            });
            Ok(CoreRunEvent::Failed {
                code: error.code,
                message: error.message,
            })
        }
        Some(pb::run_event::Payload::Accepted(_)) => Ok(CoreRunEvent::RunStarted {
            provider: String::new(),
            model: String::new(),
        }),
        None => anyhow::bail!("gRPC run event has no payload"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::pb;

    #[test]
    fn maps_message_delta_event() {
        let event = pb::RunEvent {
            run_id: "run-a".to_string(),
            request_id: "req-a".to_string(),
            session_id: "session-a".to_string(),
            sequence: 3,
            occurred_at: None,
            event_type: "message.delta".to_string(),
            payload: Some(pb::run_event::Payload::MessageDelta(pb::MessageDelta {
                delta: "hello".to_string(),
            })),
        };

        let mapped = map_grpc_event(event).unwrap();
        assert_eq!(
            mapped,
            crate::core_client::CoreRunEvent::MessageDelta {
                delta: "hello".to_string()
            }
        );
    }

    #[test]
    fn builds_create_run_request_from_core_request() {
        let request = crate::core_client::CoreRunRequest {
            request_id: "req-a".to_string(),
            session_id: "session-a".to_string(),
            actor_id: "user-a".to_string(),
            input: "hello".to_string(),
        };

        let grpc = build_create_run_request(request);
        assert_eq!(grpc.request_id, "req-a");
        assert_eq!(grpc.session_id, "session-a");
        assert_eq!(grpc.input.unwrap().text, "hello");
    }
}
