# ZeroClaw Architecture Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 产出一份基于真实代码的中文架构说明文档，能够支撑其他语言对 ZeroClaw 架构进行重实现，并重点分析 plugins 与 skills 的提供形态及其接入用户会话生命周期的可行性。

**Architecture:** 先从 CLI、库模块导出和文档约束确定系统边界，再分层阅读 Agent、Gateway、扩展点、Memory、Session、Hooks、Plugins、Skills、SkillForge 的实现。最终把整体架构、关键数据流、扩展机制、优缺点、以及 skills 生命周期接入能力整理为一份中文 Markdown 文档。

**Tech Stack:** Rust 2024, Cargo, Markdown

---

### Task 1: 建立分析边界与文档落点

**Files:**
- Read: `AGENTS.md`
- Read: `README.md`
- Read: `docs/contributing/docs-contract.md`
- Read: `src/lib.rs`
- Read: `src/main.rs`
- Create: `docs/zeroclaw_architecture.md`

- [ ] **Step 1: 确认文档约束与目标文件名**

Run: `Get-Content -Raw docs\contributing\docs-contract.md`
Expected: 明确这是新增独立文档而不是调整全站导航的任务，可以在 `docs/` 下新增英文文件名的中文内容文档。

- [ ] **Step 2: 确认系统对外入口与模块导出**

Run: `Get-Content -Raw src\lib.rs`
Expected: 识别公开模块、feature gate、核心子系统，以及 CLI 共享命令枚举。

- [ ] **Step 3: 确认 CLI 面与用户可见能力**

Run: `Get-Content -Raw src\main.rs`
Expected: 识别 `agent`、`gateway`、`daemon`、`skills`、`plugin`、`memory`、`channel`、`peripheral` 等入口。

### Task 2: 阅读核心运行架构

**Files:**
- Read: `src/agent/mod.rs`
- Read: `src/agent/agent.rs`
- Read: `src/agent/history.rs`
- Read: `src/agent/memory_loader.rs`
- Read: `src/gateway/mod.rs`
- Read: `src/config/mod.rs`
- Read: `src/config/schema.rs`
- Read: `src/providers/traits.rs`
- Read: `src/channels/traits.rs`
- Read: `src/tools/traits.rs`
- Read: `src/memory/traits.rs`
- Read: `src/observability/traits.rs`
- Read: `src/runtime/traits.rs`
- Read: `src/peripherals/traits.rs`

- [ ] **Step 1: 提炼运行时主链路**

Run: `rg -n "struct Agent|async fn run|build|dispatch|history|memory|tool" src/agent src/gateway`
Expected: 找到消息进入、上下文装载、模型调用、工具执行、输出回传、状态持久化的关键链路。

- [ ] **Step 2: 提炼 trait 化扩展点**

Run: `Get-Content -Raw src\tools\traits.rs`
Expected: 识别统一抽象层，为跨语言重实现定义核心接口责任。

- [ ] **Step 3: 记录总体架构图所需信息**

Expected: 形成“入口层、编排层、执行层、状态层、接入层、扩展层”的结构草图。

### Task 3: 重点阅读插件、skills、hooks、会话与长期记忆

**Files:**
- Read: `src/plugins/mod.rs`
- Read: `src/plugins/host.rs`
- Read: `src/plugins/wasm_tool.rs`
- Read: `src/plugins/wasm_channel.rs`
- Read: `src/skills/mod.rs`
- Read: `src/skills/creator.rs`
- Read: `src/skills/improver.rs`
- Read: `src/skillforge/mod.rs`
- Read: `src/skillforge/integrate.rs`
- Read: `src/tools/skill_tool.rs`
- Read: `src/tools/read_skill.rs`
- Read: `src/tools/sessions.rs`
- Read: `src/hooks/traits.rs`
- Read: `src/hooks/runner.rs`
- Read: `src/hooks/builtin/mod.rs`
- Read: `src/channels/session_backend.rs`
- Read: `src/channels/session_sqlite.rs`
- Read: `src/memory/mod.rs`
- Read: `src/memory/traits.rs`
- Read: `src/memory/sqlite.rs`
- Read: `src/memory/knowledge_graph.rs`

- [ ] **Step 1: 判定 plugins 的架构形态**

Run: `rg -n "extism|manifest|plugin|Wasm|channel" src/plugins src/gateway/api_plugins.rs Cargo.toml`
Expected: 确认 plugins 是基于 WASM/Extism 的宿主式扩展，而不是直接动态链接进主进程代码。

- [ ] **Step 2: 判定 skills 的架构形态**

Run: `rg -n "SKILL|skills|workspace/skills|install|audit|creator|improver" src README.md`
Expected: 确认 skills 主要是工作区文件包 + 管理命令 + 技能生成/改进流水线，不等同于 Rust trait 插件。

- [ ] **Step 3: 判定生命周期接入能力**

Run: `rg -n "hook|lifecycle|session|memory|history|auto-save|conversation" src/hooks src/gateway src/agent src/memory src/tools`
Expected: 明确现有可插入点位于 hooks、tools、memory、gateway session 和 agent history，而不是 skill 文件本身拥有原生生命周期回调。

### Task 4: 编写最终架构文档

**Files:**
- Create: `docs/zeroclaw_architecture.md`

- [ ] **Step 1: 写出面向重实现的总体架构**

Expected: 文档覆盖目标、分层、模块职责、关键接口、启动流程、消息与控制流、状态模型、扩展点、部署形态。

- [ ] **Step 2: 补充 plugins 与 skills 专题分析**

Expected: 文档明确两者的差异、边界、优缺点，以及新建 skill 是否能插入用户会话生命周期的结论与实现建议。

- [ ] **Step 3: 补充优缺点与重实现建议**

Expected: 文档为其他语言重构给出模块拆分、接口契约、事件总线、状态存储、兼容策略建议。

### Task 5: 校验与收尾

**Files:**
- Verify: `docs/zeroclaw_architecture.md`

- [ ] **Step 1: 自查是否覆盖用户要求**

Expected: 文档包含整体架构、可重实现信息、优缺点、plugins/skills 深入分析、生命周期可行性判断。

- [ ] **Step 2: 核对文件名与语言要求**

Expected: 文件名为英文、内容为中文、项目名与架构主题清晰。
