//! gRPC Server-to-Server binding for ZeroClaw agent runs.

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use anyhow::{Context as AnyhowContext, Result};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tonic::body::Body as TonicBody;
use tonic::codegen::{BoxFuture, StdError};
use tonic::metadata::MetadataMap;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use zeroclaw_api::provider::ChatMessage;
use zeroclaw_config::schema::Config;
use zeroclaw_infra::session_backend::SessionBackend;
use zeroclaw_infra::session_sqlite::SqliteSessionBackend;
use zeroclaw_runtime::agent::loop_::is_tool_loop_cancelled;
use zeroclaw_runtime::agent::{Agent, TurnEvent};
use zeroclaw_runtime::security::pairing::{PairingGuard, is_public_bind};

const PROTOCOL_VERSION: &str = "zeroclaw.v1";
const EVENT_RETAIN_LIMIT: usize = 1024;
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Manually maintained prost/tonic equivalent of `proto/zeroclaw/v1/agent.proto`.
///
/// Keeping these definitions in source avoids requiring `protoc` during normal
/// builds while the `.proto` file remains the client-facing contract.
pub mod pb {
    use super::*;
    use http::Response as HttpResponse;
    use std::sync::Arc;
    use tonic::server::{Grpc, ServerStreamingService, UnaryService};

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Actor {
        #[prost(string, tag = "1")]
        pub actor_id: String,
        #[prost(string, tag = "2")]
        pub actor_type: String,
        #[prost(string, tag = "3")]
        pub display_name: String,
        #[prost(map = "string, string", tag = "10")]
        pub metadata: HashMap<String, String>,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunInput {
        #[prost(enumeration = "run_input::InputKind", tag = "1")]
        pub kind: i32,
        #[prost(string, tag = "2")]
        pub text: String,
    }

    pub mod run_input {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum InputKind {
            Unspecified = 0,
            Message = 1,
        }
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunOptions {
        #[prost(bool, tag = "1")]
        pub stream: bool,
        #[prost(string, tag = "2")]
        pub model: String,
        #[prost(string, repeated, tag = "3")]
        pub allowed_tools: Vec<String>,
        #[prost(uint64, tag = "4")]
        pub timeout_ms: u64,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct CreateRunRequest {
        #[prost(string, tag = "1")]
        pub protocol: String,
        #[prost(string, tag = "2")]
        pub request_id: String,
        #[prost(string, tag = "3")]
        pub session_id: String,
        #[prost(message, optional, tag = "4")]
        pub actor: Option<Actor>,
        #[prost(message, optional, tag = "5")]
        pub input: Option<RunInput>,
        #[prost(message, optional, tag = "6")]
        pub options: Option<RunOptions>,
        #[prost(map = "string, string", tag = "10")]
        pub metadata: HashMap<String, String>,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct CreateRunResponse {
        #[prost(string, tag = "1")]
        pub run_id: String,
        #[prost(string, tag = "2")]
        pub request_id: String,
        #[prost(string, tag = "3")]
        pub session_id: String,
        #[prost(enumeration = "RunStatus", tag = "4")]
        pub status: i32,
        #[prost(bool, tag = "5")]
        pub duplicate: bool,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct StreamRunRequest {
        #[prost(string, tag = "1")]
        pub run_id: String,
        #[prost(uint64, tag = "2")]
        pub after_sequence: u64,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct CancelRunRequest {
        #[prost(string, tag = "1")]
        pub run_id: String,
        #[prost(string, tag = "2")]
        pub reason: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct CancelRunResponse {
        #[prost(string, tag = "1")]
        pub run_id: String,
        #[prost(bool, tag = "2")]
        pub accepted: bool,
        #[prost(enumeration = "RunStatus", tag = "3")]
        pub status: i32,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct GetRunRequest {
        #[prost(string, tag = "1")]
        pub run_id: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct GetRunResponse {
        #[prost(string, tag = "1")]
        pub run_id: String,
        #[prost(string, tag = "2")]
        pub request_id: String,
        #[prost(string, tag = "3")]
        pub session_id: String,
        #[prost(enumeration = "RunStatus", tag = "4")]
        pub status: i32,
        #[prost(uint64, tag = "5")]
        pub last_sequence: u64,
        #[prost(string, tag = "6")]
        pub final_text: String,
        #[prost(message, optional, tag = "7")]
        pub error: Option<RunError>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum RunStatus {
        Unspecified = 0,
        Accepted = 1,
        Running = 2,
        Completed = 3,
        Cancelled = 4,
        Failed = 5,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunEvent {
        #[prost(string, tag = "1")]
        pub run_id: String,
        #[prost(string, tag = "2")]
        pub request_id: String,
        #[prost(string, tag = "3")]
        pub session_id: String,
        #[prost(uint64, tag = "4")]
        pub sequence: u64,
        #[prost(message, optional, tag = "5")]
        pub occurred_at: Option<::prost_types::Timestamp>,
        #[prost(string, tag = "6")]
        pub event_type: String,
        #[prost(
            oneof = "run_event::Payload",
            tags = "10, 11, 12, 13, 14, 15, 16, 17, 18"
        )]
        pub payload: Option<run_event::Payload>,
    }

    pub mod run_event {
        #[derive(Clone, PartialEq, ::prost::Oneof)]
        pub enum Payload {
            #[prost(message, tag = "10")]
            Accepted(super::RunAccepted),
            #[prost(message, tag = "11")]
            Started(super::RunStarted),
            #[prost(message, tag = "12")]
            MessageDelta(super::MessageDelta),
            #[prost(message, tag = "13")]
            ThinkingDelta(super::ThinkingDelta),
            #[prost(message, tag = "14")]
            ToolCall(super::ToolCall),
            #[prost(message, tag = "15")]
            ToolResult(super::ToolResult),
            #[prost(message, tag = "16")]
            Completed(super::RunCompleted),
            #[prost(message, tag = "17")]
            Cancelled(super::RunCancelled),
            #[prost(message, tag = "18")]
            Failed(super::RunFailed),
        }
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunAccepted {
        #[prost(uint32, tag = "1")]
        pub queue_depth: u32,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunStarted {
        #[prost(string, tag = "1")]
        pub provider: String,
        #[prost(string, tag = "2")]
        pub model: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct MessageDelta {
        #[prost(string, tag = "1")]
        pub delta: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ThinkingDelta {
        #[prost(string, tag = "1")]
        pub delta: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ToolCall {
        #[prost(string, tag = "1")]
        pub id: String,
        #[prost(string, tag = "2")]
        pub name: String,
        #[prost(string, tag = "3")]
        pub arguments_json: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ToolResult {
        #[prost(string, tag = "1")]
        pub id: String,
        #[prost(string, tag = "2")]
        pub name: String,
        #[prost(string, tag = "3")]
        pub output: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunCompleted {
        #[prost(string, tag = "1")]
        pub final_text: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunCancelled {
        #[prost(string, tag = "1")]
        pub reason: String,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunFailed {
        #[prost(message, optional, tag = "1")]
        pub error: Option<RunError>,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct RunError {
        #[prost(string, tag = "1")]
        pub code: String,
        #[prost(string, tag = "2")]
        pub message: String,
        #[prost(bool, tag = "3")]
        pub retryable: bool,
        #[prost(map = "string, string", tag = "10")]
        pub details: HashMap<String, String>,
    }

    #[tonic::async_trait]
    pub trait AgentService: Send + Sync + 'static {
        async fn create_run(
            &self,
            request: Request<CreateRunRequest>,
        ) -> Result<Response<CreateRunResponse>, Status>;

        type StreamRunStream: Stream<Item = Result<RunEvent, Status>> + Send + 'static;

        async fn stream_run(
            &self,
            request: Request<StreamRunRequest>,
        ) -> Result<Response<Self::StreamRunStream>, Status>;

        async fn cancel_run(
            &self,
            request: Request<CancelRunRequest>,
        ) -> Result<Response<CancelRunResponse>, Status>;

        async fn get_run(
            &self,
            request: Request<GetRunRequest>,
        ) -> Result<Response<GetRunResponse>, Status>;
    }

    #[derive(Debug)]
    pub struct AgentServiceServer<T> {
        inner: Arc<T>,
    }

    impl<T> AgentServiceServer<T> {
        pub fn new(inner: T) -> Self {
            Self {
                inner: Arc::new(inner),
            }
        }
    }

    impl<T> Clone for AgentServiceServer<T> {
        fn clone(&self) -> Self {
            Self {
                inner: Arc::clone(&self.inner),
            }
        }
    }

    impl<T, B> tower_service::Service<http::Request<B>> for AgentServiceServer<T>
    where
        T: AgentService,
        B: http_body::Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = HttpResponse<TonicBody>;
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = Arc::clone(&self.inner);
            match req.uri().path() {
                "/zeroclaw.v1.AgentService/CreateRun" => {
                    struct CreateRunSvc<T: AgentService>(Arc<T>);
                    impl<T: AgentService> UnaryService<CreateRunRequest> for CreateRunSvc<T> {
                        type Response = CreateRunResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;

                        fn call(&mut self, request: Request<CreateRunRequest>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            Box::pin(async move { inner.create_run(request).await })
                        }
                    }
                    Box::pin(async move {
                        let method = CreateRunSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = Grpc::new(codec);
                        Ok(grpc.unary(method, req).await)
                    })
                }
                "/zeroclaw.v1.AgentService/StreamRun" => {
                    struct StreamRunSvc<T: AgentService>(Arc<T>);
                    impl<T: AgentService> ServerStreamingService<StreamRunRequest> for StreamRunSvc<T> {
                        type Response = RunEvent;
                        type ResponseStream = T::StreamRunStream;
                        type Future = BoxFuture<Response<Self::ResponseStream>, Status>;

                        fn call(&mut self, request: Request<StreamRunRequest>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            Box::pin(async move { inner.stream_run(request).await })
                        }
                    }
                    Box::pin(async move {
                        let method = StreamRunSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = Grpc::new(codec);
                        Ok(grpc.server_streaming(method, req).await)
                    })
                }
                "/zeroclaw.v1.AgentService/CancelRun" => {
                    struct CancelRunSvc<T: AgentService>(Arc<T>);
                    impl<T: AgentService> UnaryService<CancelRunRequest> for CancelRunSvc<T> {
                        type Response = CancelRunResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;

                        fn call(&mut self, request: Request<CancelRunRequest>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            Box::pin(async move { inner.cancel_run(request).await })
                        }
                    }
                    Box::pin(async move {
                        let method = CancelRunSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = Grpc::new(codec);
                        Ok(grpc.unary(method, req).await)
                    })
                }
                "/zeroclaw.v1.AgentService/GetRun" => {
                    struct GetRunSvc<T: AgentService>(Arc<T>);
                    impl<T: AgentService> UnaryService<GetRunRequest> for GetRunSvc<T> {
                        type Response = GetRunResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;

                        fn call(&mut self, request: Request<GetRunRequest>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            Box::pin(async move { inner.get_run(request).await })
                        }
                    }
                    Box::pin(async move {
                        let method = GetRunSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = Grpc::new(codec);
                        Ok(grpc.unary(method, req).await)
                    })
                }
                _ => Box::pin(async move {
                    let response = HttpResponse::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(TonicBody::empty())
                        .expect("static gRPC unimplemented response should build");
                    Ok(response)
                }),
            }
        }
    }

    impl<T: AgentService> tonic::server::NamedService for AgentServiceServer<T> {
        const NAME: &'static str = "zeroclaw.v1.AgentService";
    }
}

#[derive(Clone)]
pub struct GrpcAgentService {
    config: Config,
    pairing: Arc<PairingGuard>,
    registry: Arc<RunRegistry>,
    session_queue: Arc<crate::session_queue::SessionActorQueue>,
    session_backend: Option<Arc<dyn SessionBackend>>,
}

impl GrpcAgentService {
    pub fn new(config: Config) -> Result<Self> {
        let pairing = Arc::new(PairingGuard::new(
            config.gateway.require_pairing,
            &config.gateway.paired_tokens,
        ));
        let session_backend = if config.gateway.session_persistence {
            Some(Arc::new(SqliteSessionBackend::new(&config.workspace_dir)?)
                as Arc<dyn SessionBackend>)
        } else {
            None
        };

        Ok(Self {
            config,
            pairing,
            registry: Arc::new(RunRegistry::new(EVENT_RETAIN_LIMIT)),
            session_queue: Arc::new(crate::session_queue::SessionActorQueue::new(8, 30, 600)),
            session_backend,
        })
    }
}

#[tonic::async_trait]
impl pb::AgentService for GrpcAgentService {
    async fn create_run(
        &self,
        request: Request<pb::CreateRunRequest>,
    ) -> Result<Response<pb::CreateRunResponse>, Status> {
        let caller_key = authenticate_metadata(&self.pairing, request.metadata())?;
        let mut request = request.into_inner();
        let admission = self.registry.create_or_get(&caller_key, &mut request)?;

        if !admission.duplicate {
            let state = self.clone();
            let run_id = admission.run_id.clone();
            tokio::spawn(async move {
                state.execute_run(run_id).await;
            });
        }

        Ok(Response::new(pb::CreateRunResponse {
            run_id: admission.run_id,
            request_id: admission.request_id,
            session_id: admission.session_id,
            status: admission.status as i32,
            duplicate: admission.duplicate,
        }))
    }

    type StreamRunStream = Pin<Box<dyn Stream<Item = Result<pb::RunEvent, Status>> + Send>>;

    async fn stream_run(
        &self,
        request: Request<pb::StreamRunRequest>,
    ) -> Result<Response<Self::StreamRunStream>, Status> {
        authenticate_metadata(&self.pairing, request.metadata())?;
        let request = request.into_inner();
        let stream = self
            .registry
            .stream(&request.run_id, request.after_sequence)?;
        Ok(Response::new(Box::pin(stream) as Self::StreamRunStream))
    }

    async fn cancel_run(
        &self,
        request: Request<pb::CancelRunRequest>,
    ) -> Result<Response<pb::CancelRunResponse>, Status> {
        authenticate_metadata(&self.pairing, request.metadata())?;
        let request = request.into_inner();
        Ok(Response::new(
            self.registry.cancel(&request.run_id, &request.reason),
        ))
    }

    async fn get_run(
        &self,
        request: Request<pb::GetRunRequest>,
    ) -> Result<Response<pb::GetRunResponse>, Status> {
        authenticate_metadata(&self.pairing, request.metadata())?;
        let request = request.into_inner();
        self.registry
            .get(&request.run_id)
            .map(Response::new)
            .ok_or_else(|| Status::not_found("run_id not found"))
    }
}

impl GrpcAgentService {
    async fn execute_run(self, run_id: String) {
        let Some(snapshot) = self.registry.snapshot(&run_id) else {
            return;
        };

        let session_key = format!("gw_{}", snapshot.session_id);
        let queue_depth = self.session_queue.queue_depth(&session_key).await;
        self.registry.emit(
            &run_id,
            "run.accepted",
            pb::run_event::Payload::Accepted(pb::RunAccepted {
                queue_depth: queue_depth.min(u32::MAX as usize) as u32,
            }),
        );

        let _guard = match self.session_queue.acquire(&session_key).await {
            Ok(guard) => guard,
            Err(err) => {
                self.registry
                    .fail(&run_id, "session_queue", err.to_string(), true);
                return;
            }
        };

        self.registry.mark_running(&run_id);
        self.registry.emit(
            &run_id,
            "run.started",
            pb::run_event::Payload::Started(pb::RunStarted {
                provider: self
                    .config
                    .providers
                    .fallback
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                model: self
                    .config
                    .providers
                    .fallback_provider()
                    .and_then(|provider| provider.model.clone())
                    .unwrap_or_default(),
            }),
        );

        if let Some(backend) = &self.session_backend {
            let _ = backend.append(&session_key, &ChatMessage::user(&snapshot.input_text));
        }

        let mut agent = match Agent::from_config(&self.config).await {
            Ok(agent) => agent,
            Err(err) => {
                self.registry
                    .fail(&run_id, "agent_init", err.to_string(), true);
                return;
            }
        };
        agent.set_memory_session_id(Some(session_key.clone()));
        if let Some(backend) = &self.session_backend {
            agent.seed_history(&backend.load(&session_key));
        }

        let cancel_token = self.registry.cancel_token(&run_id);
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
        let input_text = snapshot.input_text.clone();
        let run_registry = Arc::clone(&self.registry);
        let event_run_id = run_id.clone();

        let turn_fut = async {
            agent
                .turn_streamed(&input_text, event_tx, cancel_token)
                .await
        };
        let forward_fut = async move {
            let mut final_text = String::new();
            while let Some(event) = event_rx.recv().await {
                if let TurnEvent::Chunk { delta } = &event {
                    final_text.push_str(delta);
                }
                let sequence = run_registry.next_sequence(&event_run_id);
                let Some(base) = run_registry.event_base(&event_run_id) else {
                    continue;
                };
                let event = map_turn_event(&base, sequence, event);
                run_registry.record_event(&event_run_id, event);
            }
            final_text
        };

        let (turn_result, final_text) = tokio::join!(turn_fut, forward_fut);
        match turn_result {
            Ok(response) => {
                let final_text = if response.is_empty() {
                    final_text
                } else {
                    response
                };
                if let Some(backend) = &self.session_backend {
                    let _ = backend.append(&session_key, &ChatMessage::assistant(&final_text));
                }
                self.registry.complete(&run_id, final_text);
            }
            Err(err) if is_tool_loop_cancelled(&err) => {
                self.registry.cancelled(&run_id, "cancelled");
            }
            Err(err) => {
                self.registry
                    .fail(&run_id, "agent_turn", err.to_string(), true);
            }
        }
    }
}

pub async fn run_grpc_server(host: &str, port: u16, config: Config) -> Result<()> {
    if is_public_bind(host) && config.tunnel.provider == "none" && !config.gateway.allow_public_bind
    {
        tracing::warn!(
            "Binding to {host}; gRPC will be exposed to all network interfaces. Use localhost, a tunnel, or gateway.allow_public_bind=true."
        );
    }

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .with_context(|| format!("invalid gRPC bind address {host}:{port}"))?;
    let service = GrpcAgentService::new(config)?;

    tracing::info!("ZeroClaw gRPC listening on {addr}");
    Server::builder()
        .add_service(pb::AgentServiceServer::new(service))
        .serve(addr)
        .await
        .context("gRPC server failed")
}

#[derive(Clone)]
struct EventBase {
    run_id: String,
    request_id: String,
    session_id: String,
}

#[derive(Clone)]
struct RunSnapshot {
    session_id: String,
    input_text: String,
}

struct RunAdmission {
    run_id: String,
    request_id: String,
    session_id: String,
    status: pb::RunStatus,
    duplicate: bool,
}

struct RunRecord {
    request_id: String,
    session_id: String,
    input_text: String,
    status: pb::RunStatus,
    sequence: u64,
    retained: VecDeque<pb::RunEvent>,
    tx: tokio::sync::broadcast::Sender<pb::RunEvent>,
    cancel_token: CancellationToken,
    final_text: String,
    error: Option<pb::RunError>,
}

struct RegistryInner {
    runs: HashMap<String, RunRecord>,
    idempotency: HashMap<(String, String), String>,
}

struct RunRegistry {
    retain_limit: usize,
    inner: Mutex<RegistryInner>,
}

impl RunRegistry {
    fn new(retain_limit: usize) -> Self {
        Self {
            retain_limit: retain_limit.max(1),
            inner: Mutex::new(RegistryInner {
                runs: HashMap::new(),
                idempotency: HashMap::new(),
            }),
        }
    }

    fn create_or_get(
        &self,
        caller_key: &str,
        request: &mut pb::CreateRunRequest,
    ) -> Result<RunAdmission, Status> {
        validate_create_run(request)?;
        let idempotency_key = (caller_key.to_string(), request.request_id.clone());
        let mut inner = self.inner.lock().expect("run registry lock poisoned");

        if let Some(run_id) = inner.idempotency.get(&idempotency_key)
            && let Some(record) = inner.runs.get(run_id)
        {
            return Ok(RunAdmission {
                run_id: run_id.clone(),
                request_id: record.request_id.clone(),
                session_id: record.session_id.clone(),
                status: record.status,
                duplicate: true,
            });
        }

        let run_id = uuid::Uuid::new_v4().to_string();
        let (tx, _rx) = tokio::sync::broadcast::channel(EVENT_CHANNEL_CAPACITY);
        inner.idempotency.insert(idempotency_key, run_id.clone());
        inner.runs.insert(
            run_id.clone(),
            RunRecord {
                request_id: request.request_id.clone(),
                session_id: request.session_id.clone(),
                input_text: request
                    .input
                    .as_ref()
                    .map_or_else(String::new, |i| i.text.clone()),
                status: pb::RunStatus::Accepted,
                sequence: 0,
                retained: VecDeque::with_capacity(self.retain_limit),
                tx,
                cancel_token: CancellationToken::new(),
                final_text: String::new(),
                error: None,
            },
        );

        Ok(RunAdmission {
            run_id,
            request_id: request.request_id.clone(),
            session_id: request.session_id.clone(),
            status: pb::RunStatus::Accepted,
            duplicate: false,
        })
    }

    fn snapshot(&self, run_id: &str) -> Option<RunSnapshot> {
        let inner = self.inner.lock().expect("run registry lock poisoned");
        inner.runs.get(run_id).map(|record| RunSnapshot {
            session_id: record.session_id.clone(),
            input_text: record.input_text.clone(),
        })
    }

    fn event_base(&self, run_id: &str) -> Option<EventBase> {
        let inner = self.inner.lock().expect("run registry lock poisoned");
        inner.runs.get(run_id).map(|record| EventBase {
            run_id: run_id.to_string(),
            request_id: record.request_id.clone(),
            session_id: record.session_id.clone(),
        })
    }

    fn next_sequence(&self, run_id: &str) -> u64 {
        let mut inner = self.inner.lock().expect("run registry lock poisoned");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return 0;
        };
        record.sequence = record.sequence.saturating_add(1);
        record.sequence
    }

    fn cancel_token(&self, run_id: &str) -> Option<CancellationToken> {
        let inner = self.inner.lock().expect("run registry lock poisoned");
        inner
            .runs
            .get(run_id)
            .map(|record| record.cancel_token.clone())
    }

    fn mark_running(&self, run_id: &str) {
        let mut inner = self.inner.lock().expect("run registry lock poisoned");
        if let Some(record) = inner.runs.get_mut(run_id) {
            record.status = pb::RunStatus::Running;
        }
    }

    fn emit(&self, run_id: &str, event_type: &str, payload: pb::run_event::Payload) {
        let sequence = self.next_sequence(run_id);
        let Some(base) = self.event_base(run_id) else {
            return;
        };
        self.record_event(
            run_id,
            pb::RunEvent {
                run_id: base.run_id,
                request_id: base.request_id,
                session_id: base.session_id,
                sequence,
                occurred_at: Some(now_timestamp()),
                event_type: event_type.to_string(),
                payload: Some(payload),
            },
        );
    }

    fn record_event(&self, run_id: &str, event: pb::RunEvent) {
        let mut inner = self.inner.lock().expect("run registry lock poisoned");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        if record.retained.len() == self.retain_limit {
            record.retained.pop_front();
        }
        record.retained.push_back(event.clone());
        let _ = record.tx.send(event);
    }

    fn complete(&self, run_id: &str, final_text: String) {
        {
            let mut inner = self.inner.lock().expect("run registry lock poisoned");
            if let Some(record) = inner.runs.get_mut(run_id) {
                record.status = pb::RunStatus::Completed;
                record.final_text = final_text.clone();
            }
        }
        self.emit(
            run_id,
            "run.completed",
            pb::run_event::Payload::Completed(pb::RunCompleted { final_text }),
        );
    }

    fn cancelled(&self, run_id: &str, reason: &str) {
        {
            let mut inner = self.inner.lock().expect("run registry lock poisoned");
            if let Some(record) = inner.runs.get_mut(run_id) {
                record.status = pb::RunStatus::Cancelled;
            }
        }
        self.emit(
            run_id,
            "run.cancelled",
            pb::run_event::Payload::Cancelled(pb::RunCancelled {
                reason: reason.to_string(),
            }),
        );
    }

    fn fail(&self, run_id: &str, code: &str, message: String, retryable: bool) {
        let error = pb::RunError {
            code: code.to_string(),
            message,
            retryable,
            details: HashMap::new(),
        };
        {
            let mut inner = self.inner.lock().expect("run registry lock poisoned");
            if let Some(record) = inner.runs.get_mut(run_id) {
                record.status = pb::RunStatus::Failed;
                record.error = Some(error.clone());
            }
        }
        self.emit(
            run_id,
            "run.failed",
            pb::run_event::Payload::Failed(pb::RunFailed { error: Some(error) }),
        );
    }

    fn cancel(&self, run_id: &str, reason: &str) -> pb::CancelRunResponse {
        let mut inner = self.inner.lock().expect("run registry lock poisoned");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return pb::CancelRunResponse {
                run_id: run_id.to_string(),
                accepted: false,
                status: pb::RunStatus::Unspecified as i32,
            };
        };

        let terminal = matches!(
            record.status,
            pb::RunStatus::Completed | pb::RunStatus::Cancelled | pb::RunStatus::Failed
        );
        if !terminal {
            record.cancel_token.cancel();
            record.status = pb::RunStatus::Cancelled;
        }
        let status = record.status as i32;
        drop(inner);

        if !terminal {
            self.emit(
                run_id,
                "run.cancelled",
                pb::run_event::Payload::Cancelled(pb::RunCancelled {
                    reason: if reason.trim().is_empty() {
                        "cancelled".to_string()
                    } else {
                        reason.to_string()
                    },
                }),
            );
        }

        pb::CancelRunResponse {
            run_id: run_id.to_string(),
            accepted: !terminal,
            status,
        }
    }

    fn get(&self, run_id: &str) -> Option<pb::GetRunResponse> {
        let inner = self.inner.lock().expect("run registry lock poisoned");
        inner.runs.get(run_id).map(|record| pb::GetRunResponse {
            run_id: run_id.to_string(),
            request_id: record.request_id.clone(),
            session_id: record.session_id.clone(),
            status: record.status as i32,
            last_sequence: record.sequence,
            final_text: record.final_text.clone(),
            error: record.error.clone(),
        })
    }

    fn stream(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> Result<impl Stream<Item = Result<pb::RunEvent, Status>> + Send + 'static, Status> {
        let (replay, rx) = {
            let inner = self.inner.lock().expect("run registry lock poisoned");
            let record = inner
                .runs
                .get(run_id)
                .ok_or_else(|| Status::not_found("run_id not found"))?;
            if let Some(first) = record.retained.front()
                && after_sequence > 0
                && after_sequence < first.sequence
            {
                return Err(Status::out_of_range(
                    "after_sequence is older than retained events",
                ));
            }
            let replay = record
                .retained
                .iter()
                .filter(|event| event.sequence > after_sequence)
                .cloned()
                .collect::<Vec<_>>();
            (replay, record.tx.subscribe())
        };

        let replay_stream = tokio_stream::iter(replay.into_iter().map(Ok));
        let live_stream = BroadcastStream::new(rx).filter_map(|item| match item {
            Ok(event) => Some(Ok(event)),
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => {
                Some(Err(Status::data_loss("run event stream lagged")))
            }
        });
        Ok(replay_stream.chain(live_stream))
    }
}

fn authenticate_metadata(guard: &PairingGuard, metadata: &MetadataMap) -> Result<String, Status> {
    if !guard.require_pairing() {
        return Ok("anonymous".to_string());
    }

    let token = metadata
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or("");

    guard
        .authenticate_and_hash(token)
        .ok_or_else(|| Status::unauthenticated("missing or invalid bearer token"))
}

fn validate_create_run(request: &mut pb::CreateRunRequest) -> Result<(), Status> {
    if request.protocol.trim().is_empty() {
        request.protocol = PROTOCOL_VERSION.to_string();
    }
    if request.protocol != PROTOCOL_VERSION {
        return Err(Status::invalid_argument("protocol must be zeroclaw.v1"));
    }
    if request.request_id.trim().is_empty() {
        return Err(Status::invalid_argument("request_id is required"));
    }
    if request.session_id.trim().is_empty() {
        return Err(Status::invalid_argument("session_id is required"));
    }
    let Some(input) = request.input.as_ref() else {
        return Err(Status::invalid_argument("input is required"));
    };
    if input.kind != pb::run_input::InputKind::Message as i32 {
        return Err(Status::invalid_argument("input.kind must be MESSAGE"));
    }
    if input.text.trim().is_empty() {
        return Err(Status::invalid_argument("input.text is required"));
    }
    Ok(())
}

fn map_turn_event(base: &EventBase, sequence: u64, event: TurnEvent) -> pb::RunEvent {
    let (event_type, payload) = match event {
        TurnEvent::Chunk { delta } => (
            "message.delta",
            pb::run_event::Payload::MessageDelta(pb::MessageDelta { delta }),
        ),
        TurnEvent::Thinking { delta } => (
            "thinking.delta",
            pb::run_event::Payload::ThinkingDelta(pb::ThinkingDelta { delta }),
        ),
        TurnEvent::ToolCall { id, name, args } => (
            "tool.call",
            pb::run_event::Payload::ToolCall(pb::ToolCall {
                id,
                name,
                arguments_json: args.to_string(),
            }),
        ),
        TurnEvent::ToolResult { id, name, output } => (
            "tool.result",
            pb::run_event::Payload::ToolResult(pb::ToolResult { id, name, output }),
        ),
    };

    pb::RunEvent {
        run_id: base.run_id.clone(),
        request_id: base.request_id.clone(),
        session_id: base.session_id.clone(),
        sequence,
        occurred_at: Some(now_timestamp()),
        event_type: event_type.to_string(),
        payload: Some(payload),
    }
}

fn now_timestamp() -> prost_types::Timestamp {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    prost_types::Timestamp {
        seconds: now.as_secs().min(i64::MAX as u64) as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataMap;

    fn text_request(request_id: &str, session_id: &str, text: &str) -> pb::CreateRunRequest {
        pb::CreateRunRequest {
            protocol: PROTOCOL_VERSION.to_string(),
            request_id: request_id.to_string(),
            session_id: session_id.to_string(),
            actor: Some(pb::Actor {
                actor_id: "user-1".to_string(),
                actor_type: "server1-user".to_string(),
                display_name: "User 1".to_string(),
                metadata: Default::default(),
            }),
            input: Some(pb::RunInput {
                kind: pb::run_input::InputKind::Message as i32,
                text: text.to_string(),
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

    #[test]
    fn validate_create_run_rejects_missing_request_id() {
        let mut request = text_request("", "session-a", "hello");
        let err = validate_create_run(&mut request).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("request_id"));
    }

    #[test]
    fn validate_create_run_rejects_missing_session_id() {
        let mut request = text_request("req-a", "", "hello");
        let err = validate_create_run(&mut request).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("session_id"));
    }

    #[test]
    fn validate_create_run_rejects_empty_text() {
        let mut request = text_request("req-a", "session-a", "   ");
        let err = validate_create_run(&mut request).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("input.text"));
    }

    #[test]
    fn authenticate_allows_disabled_pairing() {
        let guard = PairingGuard::new(false, &[]);
        let metadata = MetadataMap::new();
        let key = authenticate_metadata(&guard, &metadata).unwrap();
        assert_eq!(key, "anonymous");
    }

    #[test]
    fn authenticate_accepts_valid_bearer_token() {
        let guard = PairingGuard::new(true, &["zc_valid".to_string()]);
        let mut metadata = MetadataMap::new();
        metadata.insert("authorization", "Bearer zc_valid".parse().unwrap());
        let key = authenticate_metadata(&guard, &metadata).unwrap();
        assert_eq!(key, PairingGuard::token_hash("zc_valid"));
    }

    #[test]
    fn authenticate_rejects_invalid_bearer_token() {
        let guard = PairingGuard::new(true, &["zc_valid".to_string()]);
        let mut metadata = MetadataMap::new();
        metadata.insert("authorization", "Bearer zc_invalid".parse().unwrap());
        let err = authenticate_metadata(&guard, &metadata).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn duplicate_request_id_returns_same_run_id_for_same_caller() {
        let registry = RunRegistry::new(16);
        let mut first = text_request("req-a", "session-a", "hello");
        let first = registry.create_or_get("caller-a", &mut first).unwrap();
        let mut duplicate = text_request("req-a", "session-a", "hello again");
        let duplicate = registry.create_or_get("caller-a", &mut duplicate).unwrap();

        assert_eq!(first.run_id, duplicate.run_id);
        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
    }

    #[test]
    fn cancel_unknown_run_is_idempotent() {
        let registry = RunRegistry::new(16);
        let response = registry.cancel("missing-run", "client cancelled");
        assert_eq!(response.run_id, "missing-run");
        assert!(!response.accepted);
        assert_eq!(response.status, pb::RunStatus::Unspecified as i32);
    }

    #[test]
    fn turn_event_maps_to_protocol_event_type() {
        let base = EventBase {
            run_id: "run-a".to_string(),
            request_id: "req-a".to_string(),
            session_id: "session-a".to_string(),
        };
        let event = map_turn_event(
            &base,
            7,
            TurnEvent::ToolCall {
                id: "tool-1".to_string(),
                name: "shell".to_string(),
                args: serde_json::json!({"cmd":"pwd"}),
            },
        );

        assert_eq!(event.sequence, 7);
        assert_eq!(event.event_type, "tool.call");
        assert!(matches!(
            event.payload,
            Some(pb::run_event::Payload::ToolCall(_))
        ));
    }
}
