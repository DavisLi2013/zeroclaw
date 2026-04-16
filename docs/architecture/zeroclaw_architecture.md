# ZeroClaw 架构说明

## 1. 文档目标

本文基于当前仓库代码结构，对 ZeroClaw 做面向重实现的架构分析。目标不是解释单个 Rust 语法细节，而是抽取其稳定的系统形态、模块边界、运行时装配方式、扩展点设计、插件与 skills 子系统，并回答一个更具体的问题：

- 如果不用 Rust，是否可以用 Go、Java、Kotlin、TypeScript、Python、C# 等语言重建同等架构。
- 现有 `plugin` 与 `skills` 能力分别属于什么架构形态。
- 是否能让“新建 skill”插入用户会话生命周期，从历史会话里沉淀长期偏好。

结论先行：

- ZeroClaw 的主架构是一个典型的“端口-适配器 / Hexagonal Architecture + 运行时装配容器 + 事件/钩子切面”的代理系统。
- 它最核心的稳定抽象不是 Rust 语法，而是各类 trait：`Provider`、`Tool`、`Channel`、`Memory`、`Observer`、`RuntimeAdapter`、`Peripheral`。
- 因此它非常适合被其他语言重实现，只要保留这些抽象边界与装配顺序即可。
- `skills` 当前不是生命周期插件，而是“文件系统发现的声明式能力包”，主要通过提示词注入和工具派生生效。
- `plugins` 当前是“Manifest 驱动的 WASM 插件宿主骨架”，控制面已经存在，但数据面执行桥还没有真正接通。
- 如果目标是“让新增 skill 自动介入用户会话生命周期并分析长期偏好”，当前底座已经具备 70% 以上条件，但需要把 `skills` 与 `hooks/session/memory` 正式接线；原生状态下并不直接支持。

---

## 2. 系统定位与设计原则

ZeroClaw 可以看成一个“多入口、多能力源、多状态后端”的智能体运行时。它同时具备：

- 多入口：CLI、Gateway(Web/WS)、多聊天渠道、硬件外设。
- 多能力源：LLM Provider、Tools、Skills、Memory、Hooks、Plugins、Peripherals。
- 多状态面：对话历史、长期记忆、会话持久化、观测事件、缓存。
- 多安全边界：命令执行策略、路径约束、权限控制、签名校验、审计。

它的几个关键设计原则如下：

### 2.1 Trait-first

所有关键能力都先抽象成 trait，再通过工厂或装配函数实例化。这意味着系统的主结构不是“某个具体类”，而是“接口契约 + 组合关系”。

### 2.2 Orchestrator-centered

真正驱动系统的不是某个 provider，也不是某个工具，而是代理回合循环（agent loop）。模型、工具、记忆、技能、hook 都围绕“完成一次 turn”被编排。

### 2.3 Prompt + Tool 双轨

ZeroClaw 不把智能体仅仅理解为“提示词系统”，也不把它仅仅理解为“工具调度器”，而是把两者并列：

- 一部分能力通过系统提示词注入。
- 一部分能力通过显式工具注册。
- `skills` 则同时跨越这两条轨道。

### 2.4 安全优先

命令执行、技能脚本、插件签名、路径访问、工具操作权限，都被视为一等公民，而不是后补逻辑。

### 2.5 渐进式扩展

仓库中可以明显看到很多“先把接口占住，再逐步补实现”的设计，例如：

- `plugins` 已有宿主、清单、权限与 CLI，但执行桥未完全接通。
- `skills.skill_improvement` 已有配置与模块，但主流程接线较弱。
- `Memory` 的部分高级能力属于接口预留型设计。

这说明项目在架构上追求扩展弹性，但也带来了“骨架能力多、完全落地能力少”的现实特征。

---

## 3. 总体架构图

```text
+--------------------------------------------------------------+
|                        Interaction Layer                     |
|  CLI | Gateway HTTP/WS | Telegram/Discord/Slack/... | HW    |
+------------------------------+-------------------------------+
                               |
                               v
+--------------------------------------------------------------+
|                     Application Orchestration                |
|  Agent Loop | Prompt Builder | History | Memory Loader       |
|  Hook Runner | Session Coordinator | Tool Call Loop          |
+------------------------------+-------------------------------+
                               |
      +------------------------+------------------------+
      |                        |                        |
      v                        v                        v
+-------------+      +------------------+      +-----------------+
| LLM Ports   |      | Capability Ports |      | State Ports     |
| Provider    |      | Tool / Skill     |      | Memory / Session|
| Model route |      | Plugin / Hook    |      | Cache / Observe |
+------+------+      +---------+--------+      +--------+--------+
       |                       |                        |
       v                       v                        v
+-------------+      +------------------+      +-----------------+
| OpenAI/...  |      | Built-in tools   |      | SQLite memory   |
| Anthropic   |      | Skill tools      |      | Session store   |
| Resilient   |      | WASM plugins(*)  |      | Observer sinks  |
+-------------+      +------------------+      +-----------------+

(*) 当前插件宿主已存在，但实际 WASM 工具/渠道调用桥尚未接通
```

这个图体现了一个重要判断：ZeroClaw 并不是“围绕模型 API 的简单壳”，而是一个代理运行时内核，模型只是其中一个端口。

---

## 4. 模块分层说明

### 4.1 入口层

#### 4.1.1 CLI

`src/main.rs` 是统一命令入口，负责：

- 读取配置。
- 路由到 agent/gateway/channel/memory/skills/peripheral/plugin 等子命令。
- 在编译特性启用时暴露插件命令。

CLI 的价值不只是“命令行入口”，它还是运行时能力的显式控制面，因此很多子系统都有独立 CLI。

#### 4.1.2 Gateway

`src/gateway/` 提供 HTTP/WS 服务，是面向前端、远程客户端、浏览器 UI 的控制平面。它负责：

- 管理会话状态。
- 维护 WebSocket 回合流式输出。
- 将用户请求接入 Agent。
- 保存 session history。
- 在回合完成后触发长期记忆整合。

#### 4.1.3 Channels

`src/channels/` 负责 Telegram、Discord、Slack 等渠道适配。其职责类似 Gateway，但更接近“消息总线适配器”，负责把外部消息协议翻译为内部 `ChannelMessage`/会话事件。

#### 4.1.4 Peripherals

`src/peripherals/` 负责硬件侧能力，例如 STM32、RPi GPIO。这说明 ZeroClaw 从设计上不是纯软件聊天代理，而是能延展到具身/边缘控制场景。

### 4.2 编排层

#### 4.2.1 Agent

`src/agent/` 是系统核心，负责：

- 构造系统提示词。
- 注入工具、技能、记忆上下文。
- 驱动模型调用。
- 处理工具调用循环。
- 管理历史、缓存、安全摘要、自治级别。

这是最核心的“用例层 / application service”。

#### 4.2.2 Prompt Builder

`src/agent/prompt.rs` 不是简单字符串拼接器，而是把运行时状态转成“模型可消费的控制上下文”。它会注入：

- 时间与身份说明。
- 工具使用规约。
- 安全约束。
- skills 描述。
- 工作区与运行时信息。

因此在其他语言重实现时，必须把它视为一个独立模块，而不是散落在业务代码里的模板字符串。

#### 4.2.3 History / Context Compression

`src/agent/history.rs`、`context_compressor.rs`、`history_pruner` 等负责上下文控制。它们的意义是把“对话历史”从原始日志提升为可控资源，避免 token 无限膨胀。

#### 4.2.4 Hook Runner

`src/hooks/` 提供系统横切能力，是生命周期切面的真正承载者。其形态是：

- Void hooks：并行 fire-and-forget。
- Modifying hooks：按优先级顺序执行，可取消或改写数据。

这在架构上非常关键，因为它决定了“谁能在生命周期节点插手”。当前答案是 Hook，而不是 Skill。

### 4.3 端口层

这是 ZeroClaw 最适合跨语言迁移的部分。

#### 4.3.1 Provider

统一封装大模型能力，包括：

- chat 请求。
- 工具调用能力检测。
- 流式输出。
- 多 provider 路由。

#### 4.3.2 Tool

所有执行性能力都被建模成工具。工具是智能体的“手和脚”，也是系统最繁荣的能力面。

#### 4.3.3 Channel

所有外部消息通道都走统一通道抽象，屏蔽 Telegram/Slack/Discord 的协议差异。

#### 4.3.4 Memory

统一长期/短期记忆读写接口，是“代理是否真的会积累经验”的核心。

#### 4.3.5 Observer

统一观测与指标输出接口，用于日志、度量、审计等。

#### 4.3.6 RuntimeAdapter

把 shell、文件系统、运行命令等运行时能力从工具层进一步抽象出来，这是典型的“执行环境端口”。

#### 4.3.7 Peripheral

硬件端口抽象，使硬件板卡也能按统一方式注册到系统。

### 4.4 基础设施层

#### 4.4.1 Config

`src/config/` 提供结构化配置 schema 与默认值，是整个系统的装配驱动器。

#### 4.4.2 Memory Backends

支持 SQLite、Markdown、Lucid、Qdrant、None 等后端。当前 SQLite 方案最完整。

#### 4.4.3 Session Backends

会话持久化独立于 Memory，属于另一类状态面：

- Memory 偏向“可检索知识/长期记忆”。
- Session 偏向“按会话保存的原始对话历史”。

这是一个非常重要的架构区分。

#### 4.4.4 Plugins / Skills

二者都属于扩展能力，但形态完全不同，后文会单独展开。

---

## 5. 关键抽象及其职责

### 5.1 推荐保留的跨语言接口清单

如果要用别的语言重写，这组接口必须保留：

```text
interface Provider
interface Tool
interface Channel
interface Memory
interface Observer
interface RuntimeAdapter
interface Peripheral
interface HookHandler
interface SessionBackend
```

还需要保留的核心数据模型：

```text
ChatMessage
ChatRequest / ChatResponse
ToolCall / ToolResult
Skill / SkillTool
PluginManifest / PluginInfo
MemoryEntry / MemoryCategory
SessionMetadata / SessionState
PromptContext
```

### 5.2 为什么这些抽象是可迁移的

因为它们表达的是系统职责，而不是 Rust 技巧：

- trait 可以映射为其他语言的 interface / abstract class / protocol。
- `Arc<dyn Trait>` 可以映射为依赖注入容器中的单例接口。
- Feature flag 可以映射为模块开关或构建 Profile。
- 工厂函数可以映射为 service registry / module loader。

换言之，Rust 只是当前实现语言，不是这套架构的必要条件。

---

## 6. 运行时装配流程

### 6.1 启动装配顺序

从架构角度看，ZeroClaw 的标准装配顺序可以概括为：

```text
1. 读取 Config
2. 构建 SecurityPolicy
3. 构建 RuntimeAdapter
4. 构建 Memory backend
5. 构建 Session backend（如果启用）
6. 构建 Observer
7. 构建 Provider
8. 构建内建 Tools
9. 加载 Skills，并把一部分 skill 转换为 Tools
10. 加载 MCP/Delegation/Channel 等附加工具
11. 加载 Hooks
12. 加载 Plugins（当前主要是发现与登记）
13. 构建 Agent / Gateway / Channel Runner
14. 进入回合循环
```

### 6.2 这个装配顺序为什么重要

因为它体现了强依赖关系：

- Tool 依赖 Security、Runtime、Memory、Session。
- Prompt Builder 依赖 Skills、Tools、Security。
- Agent 依赖 Provider、Tools、Memory、Observer、Prompt Builder。
- Gateway/Channels 依赖 Agent 与 SessionBackend。

如果重写时打乱顺序，很容易导致：

- 工具注册不完整。
- prompt 中 skill 信息与实际可执行工具不一致。
- session 持久化存在但 memory consolidation 无法触发。

---

## 7. 关键运行流程

### 7.1 一次标准 Agent Turn

标准回合可以抽象为：

```text
用户输入
  -> 可选自动保存到会话/短期记忆
  -> 从 Memory 召回相关上下文
  -> 构建系统 Prompt（含 tools / skills / security / workspace）
  -> 调用 Provider
  -> 若模型请求工具调用，则进入 Tool Loop
  -> Tool 执行后写回 history
  -> 继续请求模型，直到得到最终回复
  -> 输出结果
  -> 可选触发 skill creation / memory consolidation / observability
```

这个流程说明 ZeroClaw 的核心不是“单次 LLM 调用”，而是一个状态闭环。

### 7.2 Tool Call Loop

这是 ZeroClaw 作为“代理运行时”而不是“聊天封装器”的关键：

- 模型输出工具调用意图。
- 系统根据工具注册表解析工具。
- Hook 可在工具调用前拦截或改写参数。
- 执行工具。
- 记录输出、耗时、观察事件。
- 再把结果反馈给模型。

这一循环决定了系统是否真正具备行动能力。

### 7.3 Session 持久化与长期记忆整合

当前实现里，会话状态与长期偏好沉淀并不是同一个存储层完成的，而是两段式：

#### 阶段 A：Session backend 保存原始对话

`SessionBackend` 负责：

- `load`
- `append`
- `list_sessions`
- `search`
- `set_session_state`

SQLite 版本支持：

- WAL
- FTS5 搜索
- metadata
- running/error/idle 状态跟踪

#### 阶段 B：Memory consolidation 提炼长期记忆

在 Gateway/Channels 的回合结束后，系统会异步调用 `consolidate_turn(...)`，从一轮对话中提炼：

- `history_entry`：该轮对话的摘要，写入日记型记忆。
- `memory_update`：新的事实、偏好、决策或承诺，写入长期核心记忆。

这意味着系统已经天然具备“从历史对话里提炼长期偏好”的基础路径。

### 7.4 Skills 自动生成

当启用 `skill_creation` 后，系统会在成功的多步工具执行结束后：

- 从 history 中提取工具调用记录。
- 基于任务描述生成 `SKILL.toml`。
- 写入 `workspace/skills/<slug>/`。

这属于“从执行轨迹反向沉淀可复用能力”的元编程能力。

它不是插件系统，但它是一个重要的“代理自我抽象”机制。

---

## 8. 状态模型

ZeroClaw 至少有 5 类状态，不应混淆：

### 8.1 配置状态

配置文件与 schema 决定运行时装配方式。

### 8.2 对话状态

当前 turn、上下文 history、压缩后的 prompt context。

### 8.3 Session 状态

按用户/渠道/会话键维护的原始历史与运行状态。

### 8.4 Memory 状态

长期知识、偏好、决定、每日摘要等可检索记忆。

### 8.5 扩展状态

- skills 目录中的文件状态
- plugins 目录中的 manifest/wasm 状态
- hook 的运行态
- observer 指标态

如果用其他语言重构，建议在架构上明确这 5 类状态的边界，否则很容易把“历史日志”“长期知识”“配置元数据”搅在一起。

---

## 9. 面向其他语言的重实现方案

### 9.1 推荐的包结构

如果使用其他语言，推荐保留如下逻辑分层：

```text
/core
  /model
  /ports
  /policy

/application
  /agent
  /prompt
  /hooks
  /sessions
  /memory_consolidation
  /skills
  /plugins

/adapters
  /providers
  /tools
  /channels
  /memory
  /session_backends
  /observers
  /runtime
  /peripherals

/interfaces
  /cli
  /http_gateway
  /ws_gateway

/bootstrap
  /config
  /service_container
  /factories
```

### 9.2 推荐的对象关系

```text
AppBootstrap
  -> ConfigService
  -> SecurityPolicy
  -> RuntimeAdapter
  -> MemoryFactory
  -> SessionBackendFactory
  -> ProviderFactory
  -> ToolRegistryFactory
  -> SkillLoader
  -> HookRegistry
  -> PluginHost
  -> AgentRuntime
  -> GatewayServer / ChannelRunner
```

### 9.3 关键实现建议

#### 9.3.1 保持 Tool Registry 中心化

不要把工具分散到各个功能模块各自调用。ZeroClaw 的一个核心经验是：所有可执行能力最终都应该进入统一工具注册表。

#### 9.3.2 把 Prompt Builder 当成一等模块

许多重实现失败的原因是把 prompt 拼装散落在业务逻辑里，最终难以维护。ZeroClaw 的做法更接近“提示词控制平面”，值得保留。

#### 9.3.3 Session 与 Memory 分离

这点非常关键：

- Session 保存“原始会话流”。
- Memory 保存“提炼后的长期知识”。

不要用一个表同时承担两者职责。

#### 9.3.4 Hook 不应依附于 Skill

从架构纯度上看，生命周期切面应该是 Hook 系统职责，而不应由普通 Skill 隐式承担。若要让 Skill 参与生命周期，应通过显式桥接层实现。

#### 9.3.5 插件优先做控制面，再做数据面

ZeroClaw 当前插件系统就体现了这一点：先有 manifest、权限、签名、CLI、发现机制，再补真实执行桥。这个顺序是合理的。

---

## 10. Skills 架构深度分析

### 10.1 Skills 的本质

当前 ZeroClaw 中的 `skills` 本质上是“文件系统发现的声明式能力包”，而不是动态链接插件。

它们有两种主要载体：

- `SKILL.md`
- `SKILL.toml`

它们可以包含：

- 元信息：名称、描述、版本、作者、标签。
- prompts/instructions。
- `[[tools]]` 声明。

### 10.2 Skills 的生效方式

skills 并不是直接“执行”，而是通过两条路径生效：

#### 路径 A：Prompt 注入

系统在构建 Prompt 时将 skill 信息注入模型上下文。

支持两种模式：

- `full`：完整内联 skill 指令。
- `compact`：只注入摘要，需要时通过 `read_skill(name)` 读取完整 skill 文件。

这是一种“知识性能力注入”。

#### 路径 B：工具派生

对于 `shell`、`script`、`http` 等 skill tool，系统会把它们转换成真正的 `Tool` 对象注册进工具表。

这是一种“执行性能力注入”。

因此可以把 skills 看成一种“知识 + 工具混合型扩展包”。

### 10.3 Skills 的安全形态

skills 在加载前会经过目录审计：

- 默认禁止 script-like 文件。
- 可通过配置允许脚本。
- 工具执行仍受安全策略、路径限制、速率限制约束。

这说明 skills 不是简单的“读文件即信任”，而是处于安全策略体系之内。

### 10.4 Skills 的元能力

当前代码里还存在两个更高阶方向：

#### 10.4.1 Skill Creation

把成功的多步任务自动固化为 skill。

#### 10.4.2 Skill Improvement

存在配置与模块，但从主流程接线看，并没有像 `skill_creation` 那样形成同等明确的运行时闭环，属于偏实验性能力。

### 10.5 Skills 架构优点

- 成本低：基于文件系统，易于分发与本地编辑。
- 易理解：本质上是“可读的能力包”。
- 对模型友好：既可注入上下文，也可转化为工具。
- 适合经验沉淀：支持从执行轨迹反向生成。

### 10.6 Skills 架构局限

- 缺少生命周期接口。
- 缺少结构化事件输入模型。
- 缺少稳定的用户身份视图。
- 对跨语言/跨进程扩展不如插件强。
- 更偏“代理提示与动作模板”，而不是“系统级扩展模块”。

---

## 11. Plugin 架构深度分析

### 11.1 Plugin 的本质

当前 `plugins` 是“Manifest 驱动的 WASM 插件架构骨架”。

Manifest 中声明：

- 名称、版本、描述、作者。
- `wasm_path`
- `capabilities`
- `permissions`
- 签名与发布者公钥

### 11.2 Plugin 的能力模型

当前 capability 枚举包括：

- `Tool`
- `Channel`
- `Memory`
- `Observer`

权限包括：

- `HttpClient`
- `FileRead`
- `FileWrite`
- `EnvRead`
- `MemoryRead`
- `MemoryWrite`

这说明插件系统在设计上瞄准的是“系统级扩展”，而不是仅仅补几个工具。

### 11.3 Plugin 的当前接线状态

现状可以概括为：

- 已有插件宿主 `PluginHost`
- 已有目录发现与安装/删除/列出
- 已有签名校验策略：`disabled / permissive / strict`
- 已有 CLI 与 API 列表接口
- 已能把部分插件 manifest 包装成 `WasmTool`

但关键问题是：

- `WasmTool.execute()` 仍返回“WASM execution not yet connected”
- `WasmChannel` 也仍是未真正接线状态

因此插件系统当前更像：

- 控制面：已存在
- 元数据面：已存在
- 安全面：已存在
- 实际执行数据面：未完成

### 11.4 Plugin 架构优点

- 扩展边界清晰。
- 有独立权限模型。
- 有签名验证，安全设计较完整。
- 适合未来跨语言扩展。
- WASM 天然适合把宿主与扩展能力隔离。

### 11.5 Plugin 架构局限

- 主执行桥未接通，当前更多是骨架。
- capability 虽声明了 `Memory`、`Observer`，但实际数据流整合仍较浅。
- 尚未看到面向 lifecycle hook 的 capability。
- 当前还不足以承担“核心业务扩展入口”的角色。

### 11.6 对 Plugin 的架构判断

如果说 `skills` 是“轻量声明式扩展”，那 `plugins` 就是“重型沙箱式扩展”。

二者不冲突，但分工应该非常明确：

- `skills` 负责代理行为知识和轻动作模板。
- `plugins` 负责系统级可隔离扩展。

---

## 12. Hooks 才是生命周期切面的真实入口

这是理解“新建 skill 能否插入用户会话生命周期”的关键前提。

当前系统中，真正拥有生命周期语义的是 `HookHandler`，其事件包括：

- `on_gateway_start`
- `on_gateway_stop`
- `on_session_start`
- `on_session_end`
- `on_llm_input`
- `on_llm_output`
- `on_after_tool_call`
- `on_message_sent`
- `on_heartbeat_tick`
- `before_model_resolve`
- `before_prompt_build`
- `before_llm_call`
- `before_tool_call`
- `on_message_received`
- `on_message_sending`

这说明：

- 会话生命周期已经有正式的系统抽象。
- 消息收发生命周期已经有正式切点。
- 模型调用与工具调用生命周期也已经有正式切点。

所以从架构上讲，生命周期能力并不缺，缺的是“让 skill 能声明式接入这些 hook”的桥接层。

---

## 13. 当前系统能否支持“新建 Skill 介入会话生命周期并分析长期偏好”

### 13.1 直接答案

#### 现状

原生状态下，不直接支持。

原因很明确：

- `Skill` 结构里没有 lifecycle 声明。
- `Skill` 没有实现 `HookHandler` 的接线。
- `skills` 加载流程只进入 prompt 注入和工具注册，不进入 hook 注册。
- `PluginCapability` 里也没有 `Hook` 或 `Lifecycle` 能力类型。

#### 但从架构可行性看

可以支持，而且改造成本不算高。

因为系统已经具备下列基础能力：

- SessionBackend 可保存和检索历史会话。
- Memory consolidation 已能从回合中提炼“偏好/事实/决策”。
- Hooks 已提供生命周期切面。
- Skills 已能从文件系统加载并转成能力对象。
- Tool 层已有 `sessions_list / sessions_history / sessions_send` 等会话工具。

换言之，缺的不是底层材料，而是“声明式接线”。

### 13.2 现有能力能做到什么程度

当前已经能做到：

1. 保存用户历史会话。
2. 搜索历史会话。
3. 在每轮结束后提炼长期记忆。
4. 在后续回合按相关性召回长期记忆。
5. 自动把多步操作沉淀成 skill。

所以如果你的目标是“从长期聊天中看出用户偏好”，其实现在已经部分具备，只是：

- 偏好提炼主要发生在 memory consolidation，而不是 skill 生命周期。
- 结果更偏非结构化/弱结构化文本记忆，而不是严格的用户画像模型。

### 13.3 现有能力做不到什么

当前做不到或做得不充分的点：

1. 不能声明一个 skill，在 `on_session_start` 时自动运行。
2. 不能声明一个 skill，在 `on_message_received` 时自动观察并打标签。
3. 不能声明一个 skill，在 `on_session_end` 时聚合本次会话偏好变化。
4. 不能将“长期偏好抽取”显式建模成稳定的 profile pipeline。
5. 没有统一的“跨 session 稳定 user_id -> preference profile”映射层。
6. 没有显式的隐私/同意/删除策略来约束用户画像累积。

### 13.4 建议的演进方向

如果目标是让“新建 skill”真正参与用户会话生命周期，推荐采用下面的架构升级。

#### 方案 A：给 Skill 增加 Lifecycle 扩展块

在 `SKILL.toml` 中增加类似：

```toml
[skill]
name = "user-preference-miner"
description = "从会话中提炼长期偏好"

[[lifecycle]]
event = "on_message_received"
action = "analyze_message"

[[lifecycle]]
event = "on_session_end"
action = "summarize_preferences"
```

再将其编译/映射为内部 HookHandler。

这是“让 skill 借壳 hook”的方案，最贴近当前仓库。

#### 方案 B：引入 HookSkill 新类型

不要污染普通 skill，而是单独定义：

- 普通 Skill：给模型看的能力包。
- HookSkill：给系统生命周期用的能力包。

这样职责更清晰，也更符合长期维护。

#### 方案 C：完善 Plugin，并新增 Hook Capability

如果更看重隔离性、权限和跨语言扩展，建议走插件路线：

- 完成 WASM 执行桥。
- 在 `PluginCapability` 中新增 `Hook`。
- 插件实现生命周期处理器。

这个方案更强，但改造量更大。

### 13.5 我认为最合理的做法

如果按当前项目状态，我更建议：

1. 短期：基于 `HookHandler` 增加 `HookSkill`，最快落地。
2. 中期：将偏好抽取结果写入结构化 `UserProfile` 存储。
3. 长期：完成 WASM 插件桥，把生命周期扩展升级为插件能力。

原因是：

- 现在 `skills` 已成熟到足以承担声明式配置入口。
- `hooks` 已成熟到足以承担生命周期执行入口。
- `plugins` 还没有成熟到足以承担主扩展入口。

---

## 14. 如果要支持“长期偏好分析”，建议补哪些架构件

### 14.1 增加稳定用户身份层

当前 session key 更像“渠道会话键”，但长期偏好分析需要更稳定的 `user_id` 抽象：

- 一个用户可能跨渠道。
- 一个用户可能多个 session。
- 一个 session 不等于一个用户画像。

因此需要：

```text
session_id -> actor_id -> user_profile_id
```

### 14.2 增加结构化画像模型

不要只写自由文本 memory，建议增加结构化 profile，例如：

```json
{
  "language_preferences": ["Rust", "TypeScript"],
  "communication_style": "concise",
  "risk_tolerance": "medium",
  "tool_preferences": ["cargo", "rg"],
  "domains": ["embedded", "agent runtime"],
  "confidence": 0.82,
  "evidence_refs": ["session:telegram__123#msg45", "memory:core_xxx"]
}
```

### 14.3 把偏好提炼做成显式流水线

建议流水线化：

```text
消息进入
  -> Hook/HookSkill 观察
  -> 生成 observation
  -> 写入 session evidence
  -> session end / turn end 触发聚合器
  -> 聚合成 preference candidates
  -> 冲突解决与置信度更新
  -> 写入 UserProfile / Core Memory
  -> Prompt Builder 注入简明画像摘要
```

### 14.4 增加用户画像治理机制

要支持：

- 是否允许长期偏好记忆
- 偏好删除
- 过期策略
- 敏感偏好屏蔽
- 审计追踪

否则这条能力链在产品和合规上都不完整。

---

## 15. 优缺点分析

### 15.1 整体架构优点

#### 1. 可扩展性强

trait 驱动设计让 Provider、Tool、Memory、Channel、Observer、RuntimeAdapter 都能独立替换。

#### 2. 跨语言可迁移性好

核心是接口与编排顺序，不依赖 Rust 独有语义。

#### 3. 能力组合度高

skills、tools、hooks、memory、sessions 可以组合出复杂代理行为。

#### 4. 状态层次分明

session 与 memory 分离，是非常正确的设计。

#### 5. 安全意识较强

命令执行、技能脚本、插件签名、权限控制都被前置考虑。

#### 6. 适合演进式建设

从现有代码看，很多能力可以逐步从实验特性演化到正式能力。

### 15.2 整体架构缺点

#### 1. 主编排复杂度高

agent loop、channel 流程、gateway 流程都较重，理解成本高。

#### 2. 扩展机制较多，心智负担大

tools、skills、hooks、plugins、MCP、delegate agents 都在扩展能力，但边界对新开发者并不总是清晰。

#### 3. 插件系统完成度不足

插件是战略方向，但目前执行桥未完成，容易给使用者造成“看起来支持，实际上不可用”的感受。

#### 4. Skills 与 Hooks 分裂

skills 很强，但没有生命周期；hooks 有生命周期，但不是 skills 生态的一部分。这使扩展故事不够统一。

#### 5. 部分能力存在“接口先行、实现滞后”

这对架构演进是好事，但对使用者预期管理不是好事。

#### 6. Prompt 体系与运行时耦合较深

虽然 Prompt Builder 是独立模块，但很多系统行为最终仍要靠 prompt 协调，带来一定隐式复杂度。

---

## 16. 重实现时必须保留的架构约束

如果用其他语言重写，以下约束建议视为“不可破坏”：

1. 必须保留统一工具注册表。
2. 必须保留 session 与 memory 分离。
3. 必须保留 lifecycle hooks。
4. 必须保留 prompt builder 作为独立模块。
5. 必须保留安全策略层，不可让 tool/skill/plugin 绕开它。
6. 必须保留技能加载与工具派生的双轨机制。
7. 必须把插件当作独立扩展系统，而不是普通 skill 的别名。

如果破坏这些约束，重写出来的系统就不再是 ZeroClaw 这套架构，只是“长得像”的另一套实现。

---

## 17. 建议的重实现路线图

### 第一阶段：先做最小可运行主链

- Config
- Provider
- Tool registry
- Agent loop
- Prompt builder
- SQLite memory
- SQLite session backend
- 基础 hooks

### 第二阶段：恢复 ZeroClaw 的扩展能力

- Skills 加载
- Skill -> Tool 派生
- Memory consolidation
- Observability
- Channels / Gateway

### 第三阶段：补强高级扩展

- Skill creation
- HookSkill
- 用户画像/长期偏好聚合
- WASM plugin runtime

这样能最大化复用当前架构思想，同时控制工程风险。

---

## 18. 最终判断

从架构角度看，ZeroClaw 是一个成熟度中高、完成度不完全一致的代理运行时内核：

- 主干架构成熟。
- 抽象边界清晰。
- 适合跨语言重实现。
- 扩展能力设计前瞻。
- 部分高级扩展仍处于骨架先行状态。

对于你特别关心的“插件能力”和“skills 能力”，最终判断如下：

### 18.1 Plugin

它是“系统级、可隔离、Manifest/WASM 驱动”的扩展架构，方向正确，但目前仍偏骨架。

### 18.2 Skills

它是“文件系统声明式能力包 + Prompt 注入 + Tool 派生”的混合扩展架构，当前比 plugin 更实用，也更贴近实际代理工作流。

### 18.3 是否能让新 Skill 插入用户会话生命周期并分析长期偏好

当前不能直接做到，但非常适合通过以下方式演进实现：

1. 让 Skill 声明 lifecycle。
2. 将其映射为 HookHandler。
3. 引入稳定 user profile 存储。
4. 让 session history -> observation -> preference aggregation -> memory/profile injection 成为正式流水线。

如果按这个方向演进，ZeroClaw 不仅能“记住事实”，还能逐渐形成“长期用户偏好模型”，并把它稳定反馈到后续会话行为中。
