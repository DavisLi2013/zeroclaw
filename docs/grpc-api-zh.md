# axi-agent gRPC API 文档

## 概述

本文档描述了 **axi-agent-edge** 与 **axi-agent-core** 之间使用的 gRPC 协议。此协议基于 `axi-agent.v1.AgentService`，用于创建、流式传输和管理 AI Agent 的运行会话（Run）。

**Protocol Buffers 版本**: `proto3`  
**协议版本**: `axi-agent.v1`  
**Proto 文件**: `crates/axi-agent-gateway/proto/axi-agent/v1/agent.proto`  
**传输协议**: gRPC (HTTP/2)  
**认证方式**: Bearer Token (通过 `Authorization` metadata)

---

## 服务定义

### AgentService

```protobuf
service AgentService {
  rpc CreateRun(CreateRunRequest) returns (CreateRunResponse);
  rpc StreamRun(StreamRunRequest) returns (stream RunEvent);
  rpc CancelRun(CancelRunRequest) returns (CancelRunResponse);
  rpc GetRun(GetRunRequest) returns (GetRunResponse);
}
```

---

## 1. CreateRun - 创建运行会话

创建一个新的 Agent 运行会话，用于处理用户输入。

### 请求: `CreateRunRequest`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `protocol` | string | 是 | 协议版本，必须为 `"axi-agent.v1"` |
| `request_id` | string | 是 | 客户端生成的请求唯一标识符，用于幂等性控制 |
| `session_id` | string | 是 | 会话 ID，用于会话持久化和上下文管理 |
| `actor` | Actor | 否 | 发起请求的用户/设备信息 |
| `input` | RunInput | 是 | 用户输入内容 |
| `options` | RunOptions | 否 | 运行选项配置 |
| `metadata` | map<string, string> | 否 | 自定义元数据 |

#### Actor 结构

| 字段 | 类型 | 说明 |
|------|------|------|
| `actor_id` | string | 用户/设备的唯一标识符 |
| `actor_type` | string | 类型标识，如 `"edge-user"` |
| `display_name` | string | 显示名称 |
| `metadata` | map<string, string> | 额外元数据 |

#### RunInput 结构

| 字段 | 类型 | 说明 |
|------|------|------|
| `kind` | InputKind | 输入类型，当前仅支持 `INPUT_KIND_MESSAGE` (值为 1) |
| `text` | string | 用户输入的文本内容（必填，不能为空） |

#### RunOptions 结构

| 字段 | 类型 | 说明 |
|------|------|------|
| `stream` | bool | 是否启用流式响应（建议设为 true） |
| `model` | string | 指定使用的模型（可选，空字符串表示使用默认模型） |
| `allowed_tools` | repeated string | 允许使用的工具列表（可选） |
| `timeout_ms` | uint64 | 超时时间（毫秒，0 表示无超时） |

### 响应: `CreateRunResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `run_id` | string | 服务端生成的运行会话唯一标识符（UUID 格式） |
| `request_id` | string | 回显客户端提供的 request_id |
| `session_id` | string | 回显客户端提供的 session_id |
| `status` | RunStatus | 当前运行状态 |
| `duplicate` | bool | 是否为重复请求（幂等性检测结果） |

### 幂等性说明

- 服务端使用 `(caller_key, request_id)` 作为幂等键
- 如果相同的 `request_id` 被同一客户端重复提交，将返回已存在的 `run_id`，且 `duplicate=true`
- 重复请求不会创建新的运行会话

### 示例代码 (Java)

```java
// 创建 gRPC channel
ManagedChannel channel = ManagedChannelBuilder
    .forAddress("axi-agent-core-host", 50051)
    .usePlaintext() // 生产环境请使用 TLS
    .build();

AgentServiceGrpc.AgentServiceBlockingStub stub = AgentServiceGrpc.newBlockingStub(channel)
    .withCallCredentials(new BearerTokenCallCredentials("your-bearer-token"));

// 构建请求
CreateRunRequest request = CreateRunRequest.newBuilder()
    .setProtocol("axi-agent.v1")
    .setRequestId(UUID.randomUUID().toString())
    .setSessionId("user-session-123")
    .setActor(Actor.newBuilder()
        .setActorId("user-456")
        .setActorType("edge-user")
        .setDisplayName("张三")
        .build())
    .setInput(RunInput.newBuilder()
        .setKind(RunInput.InputKind.INPUT_KIND_MESSAGE)
        .setText("你好，请帮我分析这段代码")
        .build())
    .setOptions(RunOptions.newBuilder()
        .setStream(true)
        .setTimeoutMs(120000) // 2分钟超时
        .build())
    .build();

// 发送请求
CreateRunResponse response = stub.createRun(request);
System.out.println("Run ID: " + response.getRunId());
System.out.println("Status: " + response.getStatus());
```

---

## 2. StreamRun - 流式接收运行事件

订阅指定运行会话的事件流，实时接收 Agent 的响应内容。

### 请求: `StreamRunRequest`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `run_id` | string | 是 | 运行会话 ID（由 CreateRun 返回） |
| `after_sequence` | uint64 | 否 | 从指定序列号之后开始接收事件（0 表示从头开始） |

### 响应: `stream RunEvent`

服务端返回一个事件流，每个事件包含以下字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `run_id` | string | 运行会话 ID |
| `request_id` | string | 原始请求 ID |
| `session_id` | string | 会话 ID |
| `sequence` | uint64 | 事件序列号（单调递增） |
| `occurred_at` | Timestamp | 事件发生时间 |
| `event_type` | string | 事件类型标识符 |
| `payload` | oneof | 事件负载（具体类型见下表） |

### 事件类型 (event_type 与 payload 对应关系)

| event_type | payload 类型 | 说明 |
|------------|-------------|------|
| `run.accepted` | RunAccepted | 运行已被接受，进入队列 |
| `run.started` | RunStarted | 运行开始执行 |
| `message.delta` | MessageDelta | Agent 响应的增量文本片段 |
| `thinking.delta` | ThinkingDelta | Agent 思考过程的增量文本 |
| `tool.call` | ToolCall | Agent 调用工具 |
| `tool.result` | ToolResult | 工具执行结果 |
| `run.completed` | RunCompleted | 运行成功完成 |
| `run.cancelled` | RunCancelled | 运行被取消 |
| `run.failed` | RunFailed | 运行失败 |

### Payload 结构详解

#### RunAccepted
```protobuf
message RunAccepted {
  uint32 queue_depth = 1;  // 当前队列深度
}
```

#### RunStarted
```protobuf
message RunStarted {
  string provider = 1;  // AI 提供商名称（如 "anthropic"）
  string model = 2;     // 使用的模型名称（如 "claude-opus-4-7"）
}
```

#### MessageDelta
```protobuf
message MessageDelta {
  string delta = 1;  // 响应文本的增量片段
}
```
**说明**: 客户端需要累积所有 `delta` 字段以获得完整响应。

#### ThinkingDelta
```protobuf
message ThinkingDelta {
  string delta = 1;  // 思考过程的增量片段
}
```

#### ToolCall
```protobuf
message ToolCall {
  string id = 1;              // 工具调用的唯一标识符
  string name = 2;            // 工具名称（如 "shell", "read_file"）
  string arguments_json = 3;  // 工具参数（JSON 字符串格式）
}
```

#### ToolResult
```protobuf
message ToolResult {
  string id = 1;      // 对应的 ToolCall.id
  string name = 2;    // 工具名称
  string output = 3;  // 工具执行结果
}
```

#### RunCompleted
```protobuf
message RunCompleted {
  string final_text = 1;  // 完整的最终响应文本
}
```

#### RunCancelled
```protobuf
message RunCancelled {
  string reason = 1;  // 取消原因
}
```

#### RunFailed
```protobuf
message RunFailed {
  RunError error = 1;  // 错误详情
}
```

#### RunError
```protobuf
message RunError {
  string code = 1;                    // 错误代码（如 "agent_init", "session_queue"）
  string message = 2;                 // 错误消息
  bool retryable = 3;                 // 是否可重试
  map<string, string> details = 10;   // 额外错误详情
}
```

### 事件流处理逻辑

1. **接收 `run.accepted`**: 运行已进入队列，等待执行
2. **接收 `run.started`**: 开始执行，获知使用的模型
3. **循环接收增量事件**:
   - `message.delta`: 累积文本片段
   - `thinking.delta`: 可选的思考过程
   - `tool.call` + `tool.result`: 工具调用及结果
4. **接收终止事件**:
   - `run.completed`: 成功完成，获取 `final_text`
   - `run.cancelled`: 被取消
   - `run.failed`: 执行失败

### 示例代码 (Java)

```java
// 创建异步 stub
AgentServiceGrpc.AgentServiceStub asyncStub = AgentServiceGrpc.newStub(channel)
    .withCallCredentials(new BearerTokenCallCredentials("your-bearer-token"));

StreamRunRequest streamRequest = StreamRunRequest.newBuilder()
    .setRunId(runId)
    .setAfterSequence(0)
    .build();

// 流式接收事件
asyncStub.streamRun(streamRequest, new StreamObserver<RunEvent>() {
    private StringBuilder fullResponse = new StringBuilder();
    
    @Override
    public void onNext(RunEvent event) {
        System.out.println("Event: " + event.getEventType() + " (seq=" + event.getSequence() + ")");
        
        switch (event.getEventType()) {
            case "run.started":
                RunStarted started = event.getStarted();
                System.out.println("使用模型: " + started.getModel());
                break;
                
            case "message.delta":
                MessageDelta delta = event.getMessageDelta();
                fullResponse.append(delta.getDelta());
                System.out.print(delta.getDelta()); // 实时输出
                break;
                
            case "tool.call":
                ToolCall toolCall = event.getToolCall();
                System.out.println("\n调用工具: " + toolCall.getName());
                System.out.println("参数: " + toolCall.getArgumentsJson());
                break;
                
            case "tool.result":
                ToolResult toolResult = event.getToolResult();
                System.out.println("工具结果: " + toolResult.getOutput());
                break;
                
            case "run.completed":
                RunCompleted completed = event.getCompleted();
                System.out.println("\n完整响应: " + completed.getFinalText());
                break;
                
            case "run.failed":
                RunFailed failed = event.getFailed();
                System.err.println("运行失败: " + failed.getError().getMessage());
                break;
        }
    }
    
    @Override
    public void onError(Throwable t) {
        System.err.println("流错误: " + t.getMessage());
    }
    
    @Override
    public void onCompleted() {
        System.out.println("\n事件流结束");
    }
});
```

---

## 3. CancelRun - 取消运行会话

取消正在执行的运行会话。

### 请求: `CancelRunRequest`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `run_id` | string | 是 | 要取消的运行会话 ID |
| `reason` | string | 否 | 取消原因（可选） |

### 响应: `CancelRunResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `run_id` | string | 运行会话 ID |
| `accepted` | bool | 是否接受取消请求（已完成的运行无法取消，返回 false） |
| `status` | RunStatus | 取消后的运行状态 |

### 示例代码 (Java)

```java
CancelRunRequest cancelRequest = CancelRunRequest.newBuilder()
    .setRunId(runId)
    .setReason("用户主动取消")
    .build();

CancelRunResponse cancelResponse = stub.cancelRun(cancelRequest);
if (cancelResponse.getAccepted()) {
    System.out.println("取消成功");
} else {
    System.out.println("无法取消（可能已完成）");
}
```

---

## 4. GetRun - 查询运行状态

查询指定运行会话的当前状态和结果。

### 请求: `GetRunRequest`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `run_id` | string | 是 | 运行会话 ID |

### 响应: `GetRunResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `run_id` | string | 运行会话 ID |
| `request_id` | string | 原始请求 ID |
| `session_id` | string | 会话 ID |
| `status` | RunStatus | 当前运行状态 |
| `last_sequence` | uint64 | 最后一个事件的序列号 |
| `final_text` | string | 最终响应文本（仅在 COMPLETED 状态时有值） |
| `error` | RunError | 错误信息（仅在 FAILED 状态时有值） |

### 示例代码 (Java)

```java
GetRunRequest getRequest = GetRunRequest.newBuilder()
    .setRunId(runId)
    .build();

GetRunResponse getResponse = stub.getRun(getRequest);
System.out.println("状态: " + getResponse.getStatus());
System.out.println("最后序列号: " + getResponse.getLastSequence());
if (getResponse.getStatus() == RunStatus.RUN_STATUS_COMPLETED) {
    System.out.println("最终结果: " + getResponse.getFinalText());
}
```

---

## 运行状态 (RunStatus)

| 枚举值 | 数值 | 说明 |
|--------|------|------|
| `RUN_STATUS_UNSPECIFIED` | 0 | 未指定（不应出现） |
| `RUN_STATUS_ACCEPTED` | 1 | 已接受，等待执行 |
| `RUN_STATUS_RUNNING` | 2 | 正在执行 |
| `RUN_STATUS_COMPLETED` | 3 | 成功完成 |
| `RUN_STATUS_CANCELLED` | 4 | 已取消 |
| `RUN_STATUS_FAILED` | 5 | 执行失败 |

---

## 认证机制

### Bearer Token 认证

所有 gRPC 请求必须在 metadata 中携带 `Authorization` 头：

```
Authorization: Bearer <your-token>
```

### Java 实现示例

```java
import io.grpc.CallCredentials;
import io.grpc.Metadata;
import io.grpc.Status;

public class BearerTokenCallCredentials extends CallCredentials {
    private final String token;
    
    public BearerTokenCallCredentials(String token) {
        this.token = token;
    }
    
    @Override
    public void applyRequestMetadata(RequestInfo requestInfo, Executor appExecutor, MetadataApplier applier) {
        appExecutor.execute(() -> {
            try {
                Metadata headers = new Metadata();
                Metadata.Key<String> authKey = Metadata.Key.of("authorization", Metadata.ASCII_STRING_MARSHALLER);
                headers.put(authKey, "Bearer " + token);
                applier.apply(headers);
            } catch (Throwable e) {
                applier.fail(Status.UNAUTHENTICATED.withCause(e));
            }
        });
    }
    
    @Override
    public void thisUsesUnstableApi() {
        // 标记使用了不稳定 API
    }
}
```

### 配置说明

- 如果 axi-agent-core 配置了 `gateway.require_pairing = true`，则必须提供有效的 Bearer Token
- Token 需要在 `gateway.paired_tokens` 列表中预先配置
- 如果 `require_pairing = false`，则可以匿名访问（不推荐生产环境）

---

## 完整工作流程示例

### 场景：用户发送消息并接收 Agent 响应

```java
public class axi-agentClient {
    private final ManagedChannel channel;
    private final AgentServiceGrpc.AgentServiceBlockingStub blockingStub;
    private final AgentServiceGrpc.AgentServiceStub asyncStub;
    
    public axi-agentClient(String host, int port, String token) {
        this.channel = ManagedChannelBuilder
            .forAddress(host, port)
            .usePlaintext()
            .build();
        
        CallCredentials credentials = new BearerTokenCallCredentials(token);
        this.blockingStub = AgentServiceGrpc.newBlockingStub(channel)
            .withCallCredentials(credentials);
        this.asyncStub = AgentServiceGrpc.newStub(channel)
            .withCallCredentials(credentials);
    }
    
    public void sendMessage(String sessionId, String userMessage) throws InterruptedException {
        // 1. 创建运行会话
        String requestId = UUID.randomUUID().toString();
        CreateRunRequest createRequest = CreateRunRequest.newBuilder()
            .setProtocol("axi-agent.v1")
            .setRequestId(requestId)
            .setSessionId(sessionId)
            .setActor(Actor.newBuilder()
                .setActorId("java-client-001")
                .setActorType("edge-user")
                .build())
            .setInput(RunInput.newBuilder()
                .setKind(RunInput.InputKind.INPUT_KIND_MESSAGE)
                .setText(userMessage)
                .build())
            .setOptions(RunOptions.newBuilder()
                .setStream(true)
                .build())
            .build();
        
        CreateRunResponse createResponse = blockingStub.createRun(createRequest);
        String runId = createResponse.getRunId();
        System.out.println("创建运行会话: " + runId);
        
        // 2. 流式接收响应
        CountDownLatch latch = new CountDownLatch(1);
        StreamRunRequest streamRequest = StreamRunRequest.newBuilder()
            .setRunId(runId)
            .setAfterSequence(0)
            .build();
        
        asyncStub.streamRun(streamRequest, new StreamObserver<RunEvent>() {
            private StringBuilder response = new StringBuilder();
            
            @Override
            public void onNext(RunEvent event) {
                switch (event.getEventType()) {
                    case "message.delta":
                        String delta = event.getMessageDelta().getDelta();
                        response.append(delta);
                        System.out.print(delta);
                        break;
                    case "run.completed":
                        System.out.println("\n\n完整响应: " + event.getCompleted().getFinalText());
                        latch.countDown();
                        break;
                    case "run.failed":
                        System.err.println("失败: " + event.getFailed().getError().getMessage());
                        latch.countDown();
                        break;
                }
            }
            
            @Override
            public void onError(Throwable t) {
                System.err.println("错误: " + t.getMessage());
                latch.countDown();
            }
            
            @Override
            public void onCompleted() {
                System.out.println("流结束");
                latch.countDown();
            }
        });
        
        // 等待完成
        latch.await();
    }
    
    public void shutdown() throws InterruptedException {
        channel.shutdown().awaitTermination(5, TimeUnit.SECONDS);
    }
    
    public static void main(String[] args) throws Exception {
        axi-agentClient client = new axi-agentClient(
            "localhost", 
            50051, 
            "your-bearer-token"
        );
        
        try {
            client.sendMessage("user-session-123", "你好，请介绍一下你自己");
        } finally {
            client.shutdown();
        }
    }
}
```

---

## 错误处理

### gRPC 状态码

| gRPC Code | 说明 | 处理建议 |
|-----------|------|----------|
| `UNAUTHENTICATED` | 认证失败（Token 无效或缺失） | 检查 Bearer Token 是否正确 |
| `INVALID_ARGUMENT` | 请求参数无效 | 检查必填字段和参数格式 |
| `NOT_FOUND` | run_id 不存在 | 确认 run_id 是否正确 |
| `OUT_OF_RANGE` | after_sequence 超出保留范围 | 从 0 开始重新订阅 |
| `DATA_LOSS` | 事件流滞后 | 重新订阅或降低处理延迟 |
| `UNAVAILABLE` | 服务不可用 | 实施重试机制（指数退避） |

### 常见错误代码 (RunError.code)

| code | 说明 | retryable |
|------|------|-----------|
| `agent_init` | Agent 初始化失败 | true |
| `agent_turn` | Agent 执行失败 | true |
| `session_queue` | 会话队列错误 | true |
| `unknown` | 未知错误 | false |

---

## 性能与限制

### 配置参数

- **最大消息大小**: 1 MB (`GRPC_MAX_DECODING_MESSAGE_SIZE`)
- **事件保留数量**: 1024 条 (`EVENT_RETAIN_LIMIT`)
- **事件通道容量**: 256 条 (`EVENT_CHANNEL_CAPACITY`)
- **会话队列**: 每个 session 串行执行，最多 8 个并发会话

### 会话管理

- 每个 `session_id` 对应一个独立的会话上下文
- 同一 `session_id` 的多个请求会串行执行（通过 `SessionActorQueue` 管理）
- 会话历史可持久化到 SQLite（需启用 `gateway.session_persistence`）

### 事件流注意事项

1. **事件保留**: 服务端仅保留最近 1024 条事件，超出部分会被丢弃
2. **滞后处理**: 如果客户端处理速度过慢，可能收到 `DATA_LOSS` 错误
3. **断线重连**: 使用 `after_sequence` 参数可以从指定位置恢复订阅

---

## Maven 依赖 (Java)

```xml
<dependencies>
    <!-- gRPC -->
    <dependency>
        <groupId>io.grpc</groupId>
        <artifactId>grpc-netty-shaded</artifactId>
        <version>1.60.0</version>
    </dependency>
    <dependency>
        <groupId>io.grpc</groupId>
        <artifactId>grpc-protobuf</artifactId>
        <version>1.60.0</version>
    </dependency>
    <dependency>
        <groupId>io.grpc</groupId>
        <artifactId>grpc-stub</artifactId>
        <version>1.60.0</version>
    </dependency>
    
    <!-- Protobuf -->
    <dependency>
        <groupId>com.google.protobuf</groupId>
        <artifactId>protobuf-java</artifactId>
        <version>3.25.1</version>
    </dependency>
</dependencies>

<build>
    <extensions>
        <extension>
            <groupId>kr.motd.maven</groupId>
            <artifactId>os-maven-plugin</artifactId>
            <version>1.7.1</version>
        </extension>
    </extensions>
    <plugins>
        <plugin>
            <groupId>org.xolstice.maven.plugins</groupId>
            <artifactId>protobuf-maven-plugin</artifactId>
            <version>0.6.1</version>
            <configuration>
                <protocArtifact>com.google.protobuf:protoc:3.25.1:exe:${os.detected.classifier}</protocArtifact>
                <pluginId>grpc-java</pluginId>
                <pluginArtifact>io.grpc:protoc-gen-grpc-java:1.60.0:exe:${os.detected.classifier}</pluginArtifact>
            </configuration>
            <executions>
                <execution>
                    <goals>
                        <goal>compile</goal>
                        <goal>compile-custom</goal>
                    </goals>
                </execution>
            </executions>
        </plugin>
    </plugins>
</build>
```

---

## 附录：Proto 文件完整定义

以下是完整的 `agent.proto` 文件内容，Java 开发团队可以直接使用此文件生成客户端代码：

```protobuf
syntax = "proto3";

package axi-agent.v1;

import "google/protobuf/timestamp.proto";

service AgentService {
  rpc CreateRun(CreateRunRequest) returns (CreateRunResponse);
  rpc StreamRun(StreamRunRequest) returns (stream RunEvent);
  rpc CancelRun(CancelRunRequest) returns (CancelRunResponse);
  rpc GetRun(GetRunRequest) returns (GetRunResponse);
}

message Actor {
  string actor_id = 1;
  string actor_type = 2;
  string display_name = 3;
  map<string, string> metadata = 10;
}

message RunInput {
  enum InputKind {
    INPUT_KIND_UNSPECIFIED = 0;
    INPUT_KIND_MESSAGE = 1;
  }

  InputKind kind = 1;
  string text = 2;
}

message RunOptions {
  bool stream = 1;
  string model = 2;
  repeated string allowed_tools = 3;
  uint64 timeout_ms = 4;
}

message CreateRunRequest {
  string protocol = 1;
  string request_id = 2;
  string session_id = 3;
  Actor actor = 4;
  RunInput input = 5;
  RunOptions options = 6;
  map<string, string> metadata = 10;
}

message CreateRunResponse {
  string run_id = 1;
  string request_id = 2;
  string session_id = 3;
  RunStatus status = 4;
  bool duplicate = 5;
}

message StreamRunRequest {
  string run_id = 1;
  uint64 after_sequence = 2;
}

message CancelRunRequest {
  string run_id = 1;
  string reason = 2;
}

message CancelRunResponse {
  string run_id = 1;
  bool accepted = 2;
  RunStatus status = 3;
}

message GetRunRequest {
  string run_id = 1;
}

message GetRunResponse {
  string run_id = 1;
  string request_id = 2;
  string session_id = 3;
  RunStatus status = 4;
  uint64 last_sequence = 5;
  string final_text = 6;
  RunError error = 7;
}

enum RunStatus {
  RUN_STATUS_UNSPECIFIED = 0;
  RUN_STATUS_ACCEPTED = 1;
  RUN_STATUS_RUNNING = 2;
  RUN_STATUS_COMPLETED = 3;
  RUN_STATUS_CANCELLED = 4;
  RUN_STATUS_FAILED = 5;
}

message RunEvent {
  string run_id = 1;
  string request_id = 2;
  string session_id = 3;
  uint64 sequence = 4;
  google.protobuf.Timestamp occurred_at = 5;
  string event_type = 6;

  oneof payload {
    RunAccepted accepted = 10;
    RunStarted started = 11;
    MessageDelta message_delta = 12;
    ThinkingDelta thinking_delta = 13;
    ToolCall tool_call = 14;
    ToolResult tool_result = 15;
    RunCompleted completed = 16;
    RunCancelled cancelled = 17;
    RunFailed failed = 18;
  }
}

message RunAccepted {
  uint32 queue_depth = 1;
}

message RunStarted {
  string provider = 1;
  string model = 2;
}

message MessageDelta {
  string delta = 1;
}

message ThinkingDelta {
  string delta = 1;
}

message ToolCall {
  string id = 1;
  string name = 2;
  string arguments_json = 3;
}

message ToolResult {
  string id = 1;
  string name = 2;
  string output = 3;
}

message RunCompleted {
  string final_text = 1;
}

message RunCancelled {
  string reason = 1;
}

message RunFailed {
  RunError error = 1;
}

message RunError {
  string code = 1;
  string message = 2;
  bool retryable = 3;
  map<string, string> details = 10;
}
```

### 使用此 Proto 文件生成 Java 代码

将上述内容保存为 `agent.proto` 文件，然后使用 Maven 插件（参见前文的 Maven 配置）或 `protoc` 命令生成 Java 代码：

```bash
# 使用 protoc 命令行工具
protoc --java_out=src/main/java \
       --grpc-java_out=src/main/java \
       --plugin=protoc-gen-grpc-java=/path/to/protoc-gen-grpc-java \
       agent.proto
```

---

## 联系与支持

如有问题或需要技术支持，请联系 axi-agent 开发团队。
