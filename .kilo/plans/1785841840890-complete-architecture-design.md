# NexusAOS — Complete Architecture Design

> **Status**: Design document — implementation-ready
> **Date**: 2026-08-04
> **Scope**: All crates, CLI, TUI, GUI, terminal engine, AI layer, governance, event sourcing, tool execution, SSH, RPC, MCP/ACP, testing, deployment

---

## 1. Executive Summary

NexusAOS is a **governance-first, event-sourced AI operating environment** for Ubuntu Linux. It is not a chatbot wrapper and not a monolithic agent. It is a small kernel plus well-bounded specialist services that route tasks to local models, tools, and persistent state safely.

The practical constraints are: 16 GB RAM, 6 GB VRAM, Ubuntu 26.04, local-first execution, and three chosen models. The architecture must assume model switching, not simultaneous residency. The kernel stays lightweight, models are replaceable, and every action is auditable, reversible, and permissioned.

The core design choice is: **the kernel owns truth, governance, execution, and audit; models only propose actions**.

---

## 2. Assumptions

| Assumption | Source | Status |
|---|---|---|
| Host is Ubuntu 26.04 LTS on GNOME/Wayland | brief | Fixed target |
| CPU: Intel i7-14700HX (20 cores, 28 threads, 5.5 GHz) | brief | Fixed target |
| RAM: 16 GB (14 GiB usable) | brief | Fixed target |
| GPU: NVIDIA RTX 4050 Max-Q (6 GB VRAM) | brief | Fixed target |
| Disk: NVMe SSD (937 GB) | brief | Fixed target |
| Only one or a small number of models active at once | brief | Fixed constraint |
| Local model stack: Gemma 4 12B, Qwen3-Coder 30B-A3B, Qwen3.5 9B vision | brief | Fixed target |
| System must work offline for core operations | brief | Fixed requirement |
| Rust-centered implementation preferred | brief | Fixed choice |
| Backend/model-provider abstraction must allow later migration | brief | Fixed requirement |
| PREEMPT_DYNAMIC kernel available | audit | Confirmed |
| All workers are local processes on the same host in v2 | brief §16 | Fixed scope |

---

## 3. Goals and Non-Goals

### 3.1 Goals

1. Route tasks to the right specialist model automatically.
2. Maintain governance, permissions, and auditability.
3. Support planning, coding, vision, and review workflows.
4. Support rollback, replay, checkpoints, and deterministic history.
5. Operate safely on constrained hardware.
6. Be local-first and usable offline.
7. Keep implementation maintainable and replaceable.
8. Preserve clean separation between kernel, model providers, tools, and memory.
9. Provide a terminal emulator that actually works (VT/ANSI compliance).
10. Provide a native GUI that is responsive and functional.

### 3.2 Non-Goals

1. Do not build a general-purpose desktop replacement.
2. Do not require all models to remain loaded simultaneously.
3. Do not assume cloud connectivity.
4. Do not embed business logic inside models.
5. Do not make the kernel dependent on a single runner, model vendor, or UI.
6. Do not optimize for maximum feature count over reliability.
7. Do not build a Wave Terminal feature-for-feature clone (the scope is different).
8. Do not use Electron or web-based UI (memory footprint constraint).

---

## 4. Architecture Options Considered

### Option A: One Monolithic App

**Pros**: Simpler start, less surface area.
**Cons**: Poor separation of concerns, hard to secure, hard to test, hard to evolve, brittle under model failure.
**Decision**: **Rejected**.

### Option B: Thin Orchestrator Around a Single Model Runner

**Pros**: Easy to ship.
**Cons**: Model lock-in, poor specialization, hard to support vision and coding separately, weak governance.
**Decision**: **Rejected**.

### Option C: Kernel + Specialist Models + Tool Layer + Event Bus

**Pros**: Fits the actual use case, supports governance, supports multiple models, enables replacement of backends, scales in complexity gradually.
**Cons**: More up-front design work, more internal interfaces to define.
**Decision**: **Chosen**.

### Option D: Wave Terminal Exact Clone

**Pros**: Proven UI patterns, block-based workflow.
**Cons**: 120,000+ LOC, Electron/web stack, no governance layer, no event sourcing, massive scope.
**Decision**: **Rejected** — we will take inspiration from Wave's block concepts and UI patterns, but rebuild in Rust with the NexusAOS governance kernel.

---

## 5. Chosen Architecture

### 5.1 High-Level Layers

```
┌─────────────────────────────────────────────────────────────────────┐
│                        User Interface Layer                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────────┐  │
│  │ CLI (clap)  │  │ TUI (ratatui)│  │ GUI (iced / wgpu)           │  │
│  └──────┬──────┘  └──────┬──────┘  └───────────┬─────────────────┘  │
│         │                │                     │                     │
│         └────────────────┼─────────────────────┘                     │
│                          │                                           │
├──────────────────────────┼───────────────────────────────────────────┤
│                    Kernel Control Plane                               │
│  ┌───────────────────────▼───────────────────────────────────────┐  │
│  │ Kernel (submit → classify → route → authorize → execute)      │  │
│  │  ├── PolicyEngine (deny-by-default, capability leases)        │  │
│  │  ├── TaskRouter (planner / coder / vision classification)     │  │
│  │  ├── ContextManager (token budgets, pressure awareness)       │  │
│  │  ├── ResourceMonitor (RAM/VRAM/disk/queue/backend health)     │  │
│  │  ├── Scheduler (queue depth enforcement, model-aware queuing) │  │
│  │  └── ReplayEngine (event-log replay, checkpoint restore)      │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                       │
│  ┌───────────────────────▼───────────────────────────────────────┐  │
│  │ Event Store (append-only, checksummed, replayable)            │  │
│  │  ├── JsonlEventStore (primary)                                │  │
│  │  ├── SqliteEventStore (secondary, indexed queries)            │  │
│  │  ├── SnapshotStore (compaction, checkpoint restore)           │  │
│  │  └── ProjectionStore (fast current-state views)               │  │
│  └───────────────────────────────────────────────────────────────┘  │
├───────────────────────────┬─────────────────────────────────────────┤
│                     Execution Layer                                  │
│  ┌────────────────────────▼─────────────────────────────────────┐  │
│  │ Tool Broker (policy-gated, scoped, logged)                    │  │
│  │  ├── FilesystemTool (read/write/move/delete with scope)      │  │
│  │  ├── GitTool (status/diff/commit/branch)                      │  │
│  │  ├── TerminalTool (sandboxed shell execution)                  │  │
│  │  ├── SearchFetchTool (permitted URLs/domains)                 │  │
│  │  └── DockerTool (container run/stop/inspect)                  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                       │
│  ┌────────────────────────▼─────────────────────────────────────┐  │
│  │ Model Providers (swappable, role-based)                       │  │
│  │  ├── OpenAIProvider (OpenAI-compatible API)                   │  │
│  │  ├── AnthropicProvider (Claude messages API)                  │  │
│  │  ├── OllamaProvider (local LM Studio / Ollama)                │  │
│  │  └── VisionProvider (Qwen3.5 9B structured observations)      │  │
│  └───────────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│                     Infrastructure Layer                              │
│  ┌────────────────────────▼─────────────────────────────────────┐  │
│  │ Terminal Engine (VT/ANSI parser + GPU renderer)               │  │
│  │  ├── VteParser (Alacritty vte crate, zero-allocation)        │  │
│  │  ├── TerminalModel (grid, scrollback, selection)              │  │
│  │  └── TerminalRenderer (iced_wgpu custom widget)               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                       │
│  ┌────────────────────────▼─────────────────────────────────────┐  │
│  │ IPC / RPC / MCP / ACP                                         │  │
│  │  ├── Unix socket JSON-RPC (kernel control)                    │  │
│  │  ├── MCP Server (tool access standard)                        │  │
│  │  └── ACP Adapter (IDE/agent client integration)               │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.2 Control Flow

```
User → CLI/TUI/GUI → Kernel → Router (classify) → Policy (check) → Provider (infer)
                 ↓                                                       ↓
           Event Store ← ← ← ← ← ← ← ← ← ← ← ← ← ← Tool Broker (execute)
```

### 5.3 Trust Boundaries

| Entity | Trust Level | Rationale |
|---|---|---|
| User input | **Untrusted** | Must be validated, sanitized, and policy-checked |
| Model output | **Untrusted** | Proposals only; kernel validates before action |
| Tool results | **Partially trusted** | Logged and validated; may be malformed or malicious |
| Event store | **Trusted** | Append-only, checksummed, immutable once written |
| External data | **Untrusted** | Must pass through policy and capability checks |

---

## 6. Layer-by-Layer Design

### 6.1 Kernel Control Plane

**Responsibilities**:
- Accept tasks
- Assign task IDs
- Track lifecycle state
- Enforce policy
- Request model inference
- Decide when tool execution is permitted
- Persist all state transitions
- Support rollback and replay

**Rules**:
- Kernel never acts on raw model output without validation.
- Kernel remains small and deterministic where possible.
- Kernel must survive model failure, tool failure, and UI disconnection.
- Every state change is an event.
- Every event is durable.

**Key Types**:
```rust
pub struct Kernel {
    event_store: Arc<dyn EventStore>,
    projection: Arc<RwLock<TaskProjection>>,
    policy: Arc<RwLock<PolicyEngine>>,
    provider_registry: Arc<ProviderRegistry>,
    tool_broker: Arc<ToolBroker>,
    context_manager: Arc<ContextManager>,
    resource_monitor: Arc<ResourceMonitor>,
    scheduler: Arc<Scheduler>,
    snapshot_store: Option<Arc<SnapshotStore>>,
    max_tool_output_size: usize,
    resource_budget: ResourceBudget,
    dedup_window_secs: u64,
    dedup_cache: Arc<RwLock<HashMap<TaskInput, (TaskId, DateTime<Utc>)>>>,
}

impl Kernel {
    pub async fn submit_task(&self, input: TaskInput) -> Result<TaskId, NexusError>;
    pub async fn execute_task(&self, task_id: &TaskId) -> Result<TaskOutcome, NexusError>;
    pub async fn transition_task(&self, task_id: &TaskId, new_state: TaskState) -> Result<(), NexusError>;
    pub async fn recover_incomplete_tasks(&self) -> Result<Vec<TaskId>, NexusError>;
    pub async fn task_count(&self) -> usize;
    pub async fn task_state(&self, task_id: &TaskId) -> Result<TaskState, NexusError>;
}
```

**Why this design**:
- The kernel is the **only** component that can mutate system state.
- All other components (models, tools, UI) are **proposers**.
- The event store is the **system of record**; the kernel is the **gatekeeper**.
- This matches the brief's "governance-first" requirement and the architecture's "models propose, kernel decides" rule.

### 6.2 Task Router

**Responsibilities**:
- Classify intent from task input
- Select planner, coder, or vision model
- Route ambiguous tasks to planner first
- Escalate to review when confidence is low
- Return structured `RouteDecision` with primary_role, secondary_roles, confidence

**Routing policy**:
- Architecture, planning, trade-off analysis → planner
- Code creation, refactor, bugfix, tests → coder
- Screenshots, PDFs, diagrams, UI interpretation → vision
- Mixed tasks → planner first, then coder, then review

**Why keyword + confidence scoring**:
- Local models have limited context; routing must be fast and deterministic.
- Keyword matching is transparent, testable, and doesn't require a model call.
- Confidence threshold allows escalation to planner when uncertain.

### 6.3 Policy Engine

**Responsibilities**:
- Define which actions are allowed
- Restrict filesystem scope
- Prevent unsafe shell execution
- Require confirmations for destructive operations
- Enforce least privilege

**Policy concepts**:
- Capability grant
- Scope (path, command, network)
- Trust tier (Untrusted, Low, Medium, High)
- Confirmation requirement
- Rollback requirement

**Key types**:
```rust
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
    default_decision: PolicyDecision,
}

pub struct PolicyRule {
    pub name: String,
    pub action_pattern: String,  // glob-like
    pub decision: String,        // allow / deny / confirm
    pub trust_tier: u8,
    pub description: Option<String>,
}

pub enum PolicyDecision {
    Allow,
    Deny,
    RequireConfirmation,
}
```

**Why this design**:
- Deny-by-default is the only safe posture for a system that executes tools.
- Policy rules are declarative and auditable.
- The engine is synchronous and fast — no I/O, no model calls.

### 6.4 Model Provider Abstraction

**Responsibilities**:
- Abstract over Ollama, LM Studio, OpenAI, Anthropic
- Normalize chat/inference APIs
- Report model capabilities
- Handle load/unload and context sizing

**Required interface**:
```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn stream(&self, request: CompletionRequest) -> Result<StreamHandle, ProviderError>;
    async fn health_check(&self) -> Result<(), ProviderError>;
    fn capabilities(&self) -> ModelCapabilities;
    async fn warmup(&self) -> Result<(), ProviderError>;
    async fn unload(&self) -> Result<(), ProviderError>;
}
```

**Provider registry**:
```rust
pub struct ProviderRegistry {
    providers: RwLock<HashMap<ModelRole, Arc<dyn ModelProvider>>>,
}

impl ProviderRegistry {
    pub fn register(&self, role: ModelRole, provider: Arc<dyn ModelProvider>);
    pub fn get(&self, role: ModelRole) -> Option<Arc<dyn ModelProvider>>;
    pub fn available_roles(&self) -> Vec<ModelRole>;
    pub async fn health_check_all(&self) -> HashMap<ModelRole, Result<(), ProviderError>>;
}
```

**Why this design**:
- The kernel speaks only to the provider interface, not to any specific runner.
- Providers are swappable by configuration change, not by redesign.
- Health checks use `catch_unwind` to prevent provider panics from crashing the kernel.

### 6.5 Tool Layer

**Responsibilities**:
- Filesystem read/write/move/delete
- Git status/commit/branch/diff
- Terminal command execution
- Search/fetch for external docs when permitted
- Docker container actions (v2)

**Tool principles**:
- Every tool call is explicit.
- Every tool call is logged.
- Tools have scopes.
- Destructive actions require confirmation or pre-approved policy.
- Tool return values are captured as events.

**Key types**:
```rust
pub struct ToolBroker {
    registry: RwLock<HashMap<String, Arc<dyn ToolExecutor>>>,
    policy: Arc<RwLock<PolicyEngine>>,
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, request: ToolRequest) -> Result<ToolResult, ToolError>;
    fn capabilities(&self) -> ToolCapabilities;
}

pub struct ToolRequest {
    pub action: String,
    pub args: serde_json::Value,
    pub task_id: TaskId,
    pub capability_lease: CapabilityLease,
}

pub struct ToolResult {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub artifacts: Vec<ArtifactRef>,
}
```

**Why this design**:
- The broker is the **only** entry point for tool execution.
- Every tool call passes through `PolicyEngine::evaluate()` before execution.
- Capability leases are time-bound and scoped, preventing privilege escalation.

### 6.6 Terminal Engine

**Responsibilities**:
- Parse VT/ANSI escape sequences correctly
- Maintain terminal grid state
- Handle scrollback, selection, mouse reporting
- Render to GPU via iced_wgpu or custom wgpu widget
- Manage PTY lifecycle

**Architecture**:
```
PTY Master → spawn_blocking read loop → mpsc channel → TerminalModel
                                                          ↓
                                                  VteParser (Perform trait)
                                                          ↓
                                                  TerminalModel (grid, scrollback)
                                                          ↓
                                                  TerminalRenderer (wgpu/iced)
                                                          ↓
                                                  GPU frame → screen
```

**Key design decisions**:

1. **Use `vte` crate** (Alacritty's parser) — battle-tested, zero-allocation state machine. **Rejected**: manual `match ch {}` parsing (broken for ANSI).

2. **PTY reads in `spawn_blocking`** — `portable-pty` is blocking; must not block the async runtime. **Rejected**: async PTY wrappers that don't exist or are unstable.

3. **TerminalModel is `Send + Sync`** — communicates with renderer via `mpsc` or `Arc<RwLock<>>`. **Rejected**: sharing PTY fd directly with renderer.

4. **Renderer is a custom iced widget** — uses `iced::widget::canvas` or drops to raw `wgpu` for instanced text rendering. **Rejected**: character-by-character `Text` widgets (too many draw calls).

**Why this design**:
- The `vte` parser is the industry standard for Rust terminal emulators (Alacritty, WezTerm, Kitty all use it or similar).
- `spawn_blocking` is the correct pattern for wrapping blocking PTY reads in async Rust.
- Custom wgpu rendering is necessary for performance; iced's default `Text` widget is too slow for terminal grids.

### 6.7 Event Store and Audit

**Responsibilities**:
- Append-only event log
- Event checksums
- Idempotency keys
- Snapshot writing/loading
- Projection rebuild
- Task history query

**Architecture**:
```
Kernel → EventStore::append(event) → JsonlEventStore (primary)
                                         ↓
                                  SqliteEventStore (indexed, secondary)
                                         ↓
                                  SnapshotStore (compaction)
                                         ↓
                                  ProjectionStore (fast reads)
```

**Key types**:
```rust
pub trait EventStore: Send + Sync {
    async fn append(&self, event: Event) -> Result<(), StorageError>;
    async fn read_all(&self) -> Result<Vec<Event>, StorageError>;
    async fn get_task_events(&self, task_id: &TaskId) -> Result<Vec<Event>, StorageError>;
    async fn get_all_events(&self) -> Result<Vec<Event>, StorageError>;
}

pub struct Event {
    pub id: EventId,
    pub task_id: TaskId,
    pub sequence: SequenceNumber,
    pub kind: EventKind,
    pub payload: EventPayload,
    pub metadata: EventMetadata,
    pub timestamp: DateTime<Utc>,
    pub checksum: String,
}

pub enum EventKind {
    TaskCreated,
    TaskClassified,
    TaskPlanned,
    TaskAuthorized,
    TaskExecuting,
    ToolInvoked,
    ToolResult,
    StateChanged,
    CheckpointCreated,
    PolicyDecision,
    ResourceBudgetExceeded,
    ModelResponded,
    // ... v2 additions
}
```

**Why this design**:
- Event sourcing is the **only** pattern that gives us perfect audit, replay, and rollback.
- JSONL is human-readable and append-only; SQLite provides indexed queries.
- Snapshots prevent unbounded replay time.
- Checksums ensure integrity.

### 6.8 Resource Management and Context Budgets

**Responsibilities**:
- Report RAM, VRAM, disk, queue depth, backend health
- Choose safe context size per task
- Refuse work when budgets are exceeded
- Enforce hard ceilings

**Key types**:
```rust
pub struct ResourceMonitor {
    // reports snapshots
}

pub struct SystemPressure {
    pub ram_available_mb: u64,
    pub ram_total_mb: u64,
    pub vram_available_mb: u64,
    pub vram_total_mb: u64,
    pub disk_available_gb: u64,
    pub queue_depth: usize,
}

pub struct ResourceBudget {
    pub max_ram_mb: u64,
    pub max_vram_mb: u64,
    pub max_context_tokens: usize,
    pub max_queue_depth: usize,
    pub min_disk_free_gb: u64,
}

impl ResourceBudget {
    pub fn exceeds_ram_budget(pressure: &SystemPressure, budget: &ResourceBudget) -> bool;
    pub fn exceeds_vram_budget(pressure: &SystemPressure, budget: &ResourceBudget) -> bool;
    pub fn exceeds_queue_budget(queue_depth: usize, budget: &ResourceBudget) -> bool;
    pub fn check_all(pressure: &SystemPressure, budget: &ResourceBudget) -> Vec<String>;
}
```

**Context Manager policy**:
- Simple question → 8K context
- Small code edit → 16K context
- Feature work → 32K context
- Architecture / repo reasoning → 64K context
- Only when absolutely necessary → higher, clamped by budget and model max

**Why this design**:
- The 30B coder model must be cold-loaded with hard queueing.
- Resource budgets prevent swap-thrash on 16 GB RAM.
- Context clamping prevents OOM on large documents.

### 6.9 Security and Capability System

**Responsibilities**:
- Define which actions are allowed
- Restrict filesystem scope
- Prevent unsafe shell execution
- Require confirmations for destructive operations
- Enforce least privilege

**Key types**:
```rust
pub struct CapabilitySet {
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    allowed_commands: Vec<String>,
    denied_commands: Vec<String>,
    max_file_size_bytes: u64,
}

impl CapabilitySet {
    pub fn check_path(&self, path: &Path) -> Result<(), CapabilityError>;
    pub fn check_command(&self, cmd: &str) -> Result<(), CapabilityError>;
    pub fn check_file_size(&self, size: u64) -> Result<(), CapabilityError>;
}

pub struct CapabilityLease {
    pub set: CapabilitySet,
    pub expires_at: DateTime<Utc>,
    pub task_id: TaskId,
}
```

**Trust boundaries**:
- User input: untrusted
- Model output: untrusted (proposals only)
- Tool results: partially trusted (logged and validated)
- External data: untrusted
- Event store: trusted (append-only, checksummed)

**Why this design**:
- Capability-based security is more flexible than ACLs for dynamic tool execution.
- Leases are time-bound, preventing privilege escalation across tasks.
- Deny-by-default is the only safe posture.

### 6.10 Replay and Checkpoint System

**Responsibilities**:
- Capture every state transition
- Capture every tool result
- Persist snapshots before risky actions
- Reconstruct prior system state for debugging or rollback

**Checkpoint triggers**:
- Before file writes
- Before git commits
- Before package installs
- Before destructive actions
- Before long multi-step tasks

**Key types**:
```rust
pub struct ReplayEngine {
    event_store: Arc<dyn EventStore>,
}

impl ReplayEngine {
    pub async fn replay(&self, from: Option<EventId>) -> Result<Vec<Event>, NexusError>;
    pub async fn replay_task(&self, task_id: &TaskId) -> Result<Vec<Event>, NexusError>;
}

pub struct SnapshotStore {
    base_path: PathBuf,
}

impl SnapshotStore {
    pub async fn save(&self, snapshot: &Snapshot) -> Result<(), StorageError>;
    pub async fn load_latest(&self) -> Result<Option<Snapshot>, StorageError>;
    pub async fn list(&self) -> Result<Vec<SnapshotMetadata>, StorageError>;
    pub async fn compact(&self, keep_last: usize) -> Result<(), StorageError>;
}
```

**Why this design**:
- Replay from event log is the only way to achieve deterministic recovery.
- Snapshots prevent replaying thousands of events on startup.
- Checkpoints before risky actions enable rollback without manual intervention.

---

## 7. Component-by-Component Design

### 7.1 CLI

**Commands**:
- `nexusaos init` — initialize configuration
- `nexusaos doctor` — check system health, models, tools
- `nexusaos status` — show kernel state, queue depth, resource pressure
- `nexusaos plan <task>` — submit task for planning only
- `nexusaos run <task>` — submit and execute task
- `nexusaos replay <task-id>` — replay task events
- `nexusaos tools list` — list available tools
- `nexusaos models list` — list registered models

**Why clap**:
- Mature, well-documented, supports subcommands and shell completions.
- No runtime overhead compared to custom parsing.

### 7.2 TUI (ratatui)

**Purpose**: Terminal-based UI for kernel operations when GUI is not available or for remote sessions.

**Layout**:
- Top bar: task queue, resource pressure indicators
- Main area: task output, tool results, model responses
- Bottom bar: status line, model indicator

**Why ratatui**:
- Mature Rust TUI library with crossterm backend.
- Works over SSH and in any terminal emulator.
- No GPU dependency.

### 7.3 GUI (iced)

**Purpose**: Native desktop UI with terminal blocks, AI chat, settings.

**Layout**:
- Sidebar: block type selector, settings
- Main area: tabbed blocks (terminal, AI chat, code editor, markdown, diff, process viewer)
- Title bar: current path, model indicator, resource gauges

**Why iced**:
- Pure Rust, no Electron overhead.
- wgpu rendering for GPU acceleration.
- Cross-platform (Linux, macOS, Windows).
- Elm-like reactive architecture is maintainable.

**Why NOT Electron/Wave's React stack**:
- Electron adds ~500 MB RAM overhead.
- Web stack adds complexity (JS/Rust bridge, webview quirks).
- NexusAOS target is ~20 MB RAM; Electron defeats the purpose.

### 7.4 Terminal Emulator (the hardest part)

**Current state** (from honest_audit.md):
- `terminal.rs` uses manual `match ch {}` parsing — broken for ANSI
- Ctrl+C broken for uppercase characters
- Enter sends `\n` instead of `\r`
- O(N) scrollback buffer shift
- RPC `id` rejects integer IDs (JSON-RPC 2.0 violation)
- `stop()` race condition

**Fix plan**:

1. **Replace manual parser with `vte` crate** (Alacritty's parser)
   - Implements Paul Williams' ANSI state machine
   - Zero-allocation
   - Battle-tested in Alacritty, WezTerm, Kitty

2. **Fix PTY input handling**
   - `Ctrl+C`: `(c.to_ascii_lowercase() as u8) & 0x1F`
   - `Enter`: send `\r` (0x0D), not `\n`
   - Use `VecDeque` for scrollback (O(1) pop_front)

3. **Fix RPC message types**
   - `id: Option<RpcId>` where `RpcId` is `#[serde(untagged)] enum RpcId { Num(i64), Str(String) }`

4. **Fix `stop()` race**
   - Use `AtomicU8` with `compare_exchange` for status

5. **Wire `nexusaos-blockctl` into GUI**
   - ShellController → TerminalBlock
   - RemoteShellController → RemoteBlock

6. **Performance optimizations**
   - Per-line damage tracking (only redraw changed lines)
   - Backpressure via `mpsc` channel with capacity limit
   - Chunked PTY reads (64KB per lock hold)
   - Glyph caching for text rendering

**Why this approach**:
- The `vte` parser is the **only** correct way to handle ANSI in Rust.
- Manual parsing is fundamentally broken for multi-byte escape sequences.
- These fixes are prerequisite to any usable terminal emulator.

---

## 8. State Model / Lifecycle

### 8.1 Task State Machine

```
received → classified → planned → authorized → executing → blocked/failed/completed → archived
                                              ↓
                                         rolled_back (compensating)
```

**State properties**:
- State transitions are explicit.
- Transitions are append-only events.
- Current state is derived from event history.

### 8.2 Manifest Lifecycle

```
draft → validated → signed → active → superseded → retired
```

**Rules**:
- Manifests are immutable once active.
- Validation checks schema and policy compliance.
- Signing uses HMAC-SHA256 (no PKI for v1).

---

## 9. Failure Modes and Edge Cases

### 9.1 Model Failures

| Failure | Mitigation |
|---|---|
| Model timeout | Retry with smaller context; route to fallback model |
| Malformed output | Kernel validates structure before action |
| Hallucinated tool usage | Policy engine rejects unauthorized tool calls |
| Model refuses task | Kernel records refusal, returns error to user |
| Slow model load | Resource budget queuing; user notification |
| Context window exceeded | Context manager clamps request; retry with smaller context |

### 9.2 Tool Failures

| Failure | Mitigation |
|---|---|
| Command exits nonzero | Error classification, retry with confirmation |
| Filesystem permission denied | Preflight check via CapabilitySet |
| Path missing | Preflight existence check |
| Git conflict | Staged execution, rollback checkpoint |
| Network unavailable | Offline mode; skip remote fetch |
| Disk full | Resource budget refusal before task starts |

### 9.3 System Failures

| Failure | Mitigation |
|---|---|
| App crash | Event sourcing recovery on restart |
| Power loss | Event sourcing + WAL journaling |
| Reboot during execution | Startup recovery scan, replay from event log |
| Corrupted cache | Rebuild projection from events |
| Backend unavailable | Health check + fallback provider |

### 9.4 User-Caused Edge Cases

| Failure | Mitigation |
|---|---|
| Ambiguous request | Clarification prompt from kernel |
| Conflicting instructions | Policy escalation to user |
| Destructive request without confirmation | `PolicyDecision::RequireConfirmation` |
| Rapid repeated prompts | Task deduplication (dedup_window_secs) |
| Model switch mid-task | Kernel pauses, switches model, resumes |

---

## 10. Security and Privacy

### 10.1 Security Rules

1. Capabilities are explicit.
2. Shell access is sandboxed or heavily constrained.
3. Filesystem access is scoped.
4. Destructive operations require confirmation.
5. Secrets are never passed to models unless absolutely necessary and redacted when possible.
6. Audit logs must not leak sensitive data unnecessarily.

### 10.2 Privacy

1. Local-first by default.
2. No cloud telemetry in the core system.
3. External model providers are optional adapters, not core dependencies.

### 10.3 Secret Redaction

```rust
pub fn redact_secrets(text: &str) -> String {
    let patterns = [
        (r"(?i)api[_-]?key\s*[:=]\s*\S+", "***REDACTED***"),
        (r"(?i)password\s*[:=]\s*\S+", "***REDACTED***"),
        (r"(?i)token\s*[:=]\s*\S+", "***REDACTED***"),
        (r"sk-[a-zA-Z0-9]{32,}", "***REDACTED***"),
    ];
    // Apply patterns before logging to event store
}
```

---

## 11. Performance and Resource Analysis

### 11.1 Constraints

| Resource | Budget | Notes |
|---|---|---|
| RAM | 14 GiB usable | 6 GB VRAM shared |
| VRAM | 6 GB | RTX 4050 Max-Q |
| Disk | 535 GB free | NVMe SSD |
| CPU | 20 cores / 28 threads | PREEMPT_DYNAMIC |

### 11.2 Strategy

1. **Load one specialist at a time** when possible (not concurrent residency).
2. **Keep kernel memory footprint small** (~20 MB).
3. **Cache summaries, not huge raw histories**.
4. **Use event sourcing plus derived views**.
5. **Avoid permanent background residency for all models**.

### 11.3 Bottlenecks

| Bottleneck | Mitigation |
|---|---|
| Model loading time | Cold-load queue, user notification, progress indicator |
| Context growth | Context manager clamps before inference |
| File diff size | Chunked diff, streaming |
| Large logs | Snapshot compaction, log rotation |
| Tool invocation latency | Preflight checks, parallel capability validation |
| Repeated review loops | Review result caching, early termination |

### 11.4 Trade-offs

| Trade-off | Decision |
|---|---|
| Better quality models can be slower | Accept; route based on task importance, not just speed |
| Faster models may be used for lightweight tasks | Accept; use smaller/faster models for prefiltering |
| Kernel should route based on task importance | Accept; confidence threshold determines escalation |

---

## 12. Testing and Validation Plan

### 12.1 Unit Tests

- Task classification
- Policy decisions
- State transitions
- Event serialization
- Rollback logic
- Provider abstraction
- Terminal parser (VTE test vectors)
- Resource budget checks
- Secret redaction

### 12.2 Integration Tests

- Model call to tool call loop
- Filesystem write and rollback
- Git commit and revert
- Vision-to-text-to-action flow
- Terminal PTY output round-trip
- SSH remote execution
- RPC request/response
- MCP tool invocation
- ACP capability grant

### 12.3 Failure Tests

- Backend unavailable
- Tool returns error
- Write permission denied
- Disk full
- Cancellation mid-task
- Task replay after crash
- Model timeout
- PTY connection lost
- RPC malformed request

### 12.4 Acceptance Criteria

1. Tasks are traceable (every state change is an event).
2. Actions are reversible where promised (snapshots before risky actions).
3. No silent destructive actions (all require confirmation or policy grant).
4. System recovers from restarts (event sourcing + replay).
5. Router chooses specialists consistently (deterministic keyword matching).

### 12.5 Terminal-Specific Tests

- VT100/ANSI compliance test vectors (from `vte` crate test suite)
- Input latency measurement (< 16ms target)
- Scrollback buffer correctness
- Mouse reporting protocol
- Alternate screen buffer
- Unicode / wide character handling

---

## 13. Implementation Phases

### Phase 0 — Repository and Safety Rails

**Goal**: Clean Rust workspace, enforce style, CI.

**Steps**:
1. Create workspace Cargo.toml with all crates.
2. Add `rustfmt.toml`, `clippy.toml`.
3. Add CI: `cargo fmt --check`, `cargo clippy`, `cargo test`.
4. Add `README.md` with project purpose.
5. Add `docs/architecture.md` with design summary.
6. Add basic test scripts.

**Done when**: `cargo test` runs, `cargo fmt --check` passes, `cargo clippy` passes.

### Phase 1 — Domain Types

**Goal**: Stable language of the system before behavior.

**Files**:
```
src/
├── lib.rs
├── config.rs
├── task.rs
├── state.rs
├── events.rs
├── manifest.rs
├── capability.rs
├── policy.rs
```

**Done when**: Every type compiles, has serialization, unit tests confirm defaults.

### Phase 2 — Event Store and Replay Spine

**Goal**: Event log as source of truth.

**Files**:
```
src/storage/
├── mod.rs
├── event_store.rs
├── snapshot.rs
├── artifact_store.rs
└── projection.rs
```

**Done when**: Task can be written, events read back, projection rebuilt, restart doesn't lose history.

### Phase 3 — Kernel Runtime and Scheduler

**Goal**: Trusted control plane.

**Files**:
```
src/runtime/
├── mod.rs
├── kernel.rs
├── scheduler.rs
├── replay.rs
└── rollback.rs
```

**Done when**: Kernel accepts request, state transitions valid, rejects unsafe tasks, restart replays state.

### Phase 4 — Resource Control and Dynamic Context

**Goal**: Stop guessing memory use.

**Files**:
```
src/
├── context.rs
└── resource.rs
```

**Done when**: Every request gets a context budget, oversized budgets clamped, pressure causes refusal.

### Phase 5 — Model Provider Abstraction and Routing

**Goal**: Swappable models without kernel changes.

**Files**:
```
src/model/
├── mod.rs
├── provider.rs
├── registry.rs
├── gemma.rs
├── coder.rs
└── vision.rs
src/router.rs
```

**Done when**: Providers swappable by config, role resolved to provider, one heavy model loads cleanly.

### Phase 6 — Tools and Tool Broker

**Goal**: Models act only through approved tools.

**Files**:
```
src/tools/
├── mod.rs
├── broker.rs
├── filesystem.rs
├── git.rs
├── terminal.rs
├── search_fetch.rs
└── docker.rs
```

**Done when**: Read-only filesystem works, git status/diff works, terminal execution gated, tool failures recoverable.

### Phase 7 — Terminal Engine (Critical Path)

**Goal**: Make the terminal actually work.

**Files**:
```
src/terminal/
├── mod.rs
├── parser.rs      // vte wrapper
├── model.rs       // grid, scrollback, selection
├── renderer.rs    // iced_wgpu custom widget
└── pty.rs         // portable-pty wrapper
```

**Done when**: VT100 compliance verified, ANSI colors work, input latency < 16ms, scrollback works.

### Phase 8 — CLI, TUI, GUI

**Goal**: Make the system inspectable and usable.

**Files**:
```
src/cli/
├── mod.rs
├── doctor.rs
├── init.rs
├── status.rs
├── plan.rs
├── run.rs
└── replay.rs
src/main.rs
```

**Done when**: CLI is clean, each command maps to one kernel action, errors are helpful.

### Phase 9 — IPC, RPC, MCP, ACP

**Goal**: External control and standardized tool access.

**Files**:
```
src/ipc/
├── mod.rs
├── rpc.rs
├── mcp.rs
└── acp.rs
```

**Done when**: JSON-RPC works over Unix socket, MCP tools pass through policy, ACP clients get capabilities.

### Phase 10 — Tests, Hardening, Release Readiness

**Goal**: Safe enough to trust.

**Steps**:
1. All unit tests pass.
2. All integration tests pass.
3. Terminal parser passes VT100 test vectors.
4. 0 clippy warnings.
5. 0 compilation warnings.
6. No silent writes.
7. No unlogged state changes.
8. No tool bypass.
9. No model bypass.
10. No uncaught task panic.
11. No schema drift without versioning.

---

## 14. MCP and ACP Integration

### 14.1 MCP (Model Context Protocol)

**Role**: MCP extends **tool access**.

**Design**:
- Add `crates/nexusaos-mcp/` crate with MCP protocol support.
- Implement MCP tool adapter that wraps existing `ToolExecutor` implementations.
- All MCP requests pass through `PolicyEngine::evaluate()` before tool execution.
- All MCP requests pass through `CapabilitySet` checks.
- MCP is a transport/interface standard, not a source of authority.

**Why MCP**:
- Standardized tool interface (Anthropic-led, Linux Foundation governed).
- Reduces N×M integration problem.
- Future-proofs tool ecosystem.

### 14.2 ACP (Agent Client Protocol)

**Role**: ACP extends **agent/client interaction**.

**Design**:
- Add `crates/nexusaos-acp/` crate with ACP protocol support.
- Implement ACP session management with capability granting.
- ACP client authentication and capability verification.
- ACP requests flow through Kernel (never bypass governance).
- ACP-connected agents receive explicit capabilities before acting.

**Why ACP**:
- JetBrains and other IDEs are adopting ACP.
- Provides standardized agent-client boundary.
- Keeps kernel as authority, IDE as client.

---

## 15. Worker Isolation (v2)

**Role**: Tools run as same-machine isolated workers with explicit capability leases.

**Design**:
- Refactor `ToolExecutor` trait to support isolated worker processes.
- Worker process management (spawn, monitor, terminate).
- Capability lease passing to workers.
- Worker health monitoring and restart logic.
- Same-machine local execution (not networked microservices).

**Why workers**:
- Prevents tool crashes from killing kernel.
- Enables capability revocation without kernel restart.
- Matches brief §16 refinement requirement.

---

## 16. Manifest Lifecycle (v2)

**Role**: Agent definitions with formal lifecycle.

**Design**:
- `Manifest` struct with lifecycle states: `draft → validated → signed → active → superseded → retired`.
- State transition validation.
- Manifest persistence (event store or separate store).
- Manifest validation logic (schema, policy compliance).
- Manifest signing/verification (HMAC-SHA256 for v1).
- Immutable once active.

**Why manifests**:
- Enables versioned, auditable agent configurations.
- Supports rollback to previous manifest.
- Matches brief §16 refinement requirement.

---

## 17. What Was Rejected and Why

| Feature | Why Rejected |
|---|---|
| Electron / web-based GUI | 500 MB RAM overhead; defeats local-first goal |
| Wave Terminal exact clone | 120K LOC, different scope, no governance layer |
| Pure microkernel (Mach-style) | IPC overhead too high for local execution |
| Cloud-first architecture | Brief requires offline-first |
| Single-model runner | Lock-in, poor specialization, weak governance |
| In-process tools | No isolation, no capability revocation, crash risk |
| Manual ANSI parsing | Fundamentally broken for multi-byte sequences |
| `INSERT OR REPLACE` in event store | Silently masks bugs in event ordering |
| `edition = "2024"` | Too new; ecosystem not ready; locked to nightly |

---

## 18. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Terminal emulator never reaches parity | Medium | High | Use `vte` crate (battle-tested); scope v1 to essentials |
| Model switching is too slow | Medium | Medium | Preload next model on idle; show progress to user |
| Event store grows unbounded | Medium | Medium | Snapshot compaction + log rotation |
| Policy engine is too restrictive | Low | Medium | Configurable trust tiers; confirmation gates |
| GPU rendering is too slow | Low | High | Profile with `tracing`; fall back to software rendering |
| MCP/ACP standards change | Medium | Low | Abstract behind traits; pin to specific versions |
| Worker isolation adds complexity | Medium | Medium | Phase after core is stable; reuse `ToolExecutor` trait |

---

## 19. Open Questions

1. **Terminal rendering**: Should we use `iced::widget::canvas` or drop to raw `wgpu` for the terminal grid? Raw wgpu is faster but more code.
2. **MCP vs ACP priority**: Which should be implemented first? MCP is more urgent for tool access; ACP is more urgent for IDE integration.
3. **Vision provider in v1**: Should we use a stub or a functional OpenAI-compatible vision provider? The brief says v1 can use a stub.
4. **Checkpoint frequency**: Every tool call, only for destructive actions, or configurable?
5. **Manifest signing**: None, simple HMAC, or full PKI? HMAC-SHA256 for v1.
6. **Default resource budgets**: What are the exact ceilings for i7-14700HX + RTX 4050 + 14 GB RAM? Needs measurement.
7. **Worker isolation mechanism**: Subprocess spawning or worker pool pattern? Subprocess for maximum isolation.

---

## 20. References

### Architecture Documents
- `nexus_aos_architecture_brief.md` — NexusAOS v2 Architecture Brief
- `docs/architecture.md` — Quick reference
- `nexus_aos_implementation_plan.md` — Build order
- `.kilo/plans/1785698871958-architecture-brief-completion.md` — v2 gaps
- `.kilo/plans/1785841840890-arc-brief-v1-implementation.md` — v1 gaps
- `.kilo/plans/1785686213157-kernel-decomposition-and-fixes.md` — Completed fixes

### External References
- [Alacritty / vte parser](https://github.com/alacritty/vte) — ANSI state machine
- [Paul Williams' ANSI parser spec](https://vt100.net/emu/dec_ansi_parser) — Reference state machine
- [MCP Specification](https://modelcontextprotocol.io/specification/2025-03-26) — Model Context Protocol
- [Anthropic Agent Design](https://docs.anthropic.com/claude/docs/agents) — Agent architecture patterns
- [OpenAI Agents SDK](https://platform.openai.com/docs/agents) — Agent patterns
- [NIST AI RMF](https://www.nist.gov/itl/ai-risk-management-framework) — Governance framework
- [OWASP Agentic AI Top Ten](https://genai.owasp.org/2025/12/09/owasp-top-10-for-agentic-applications) — Security taxonomy
- [Event Sourcing Pattern](https://microservices.io/patterns/data/event-sourcing.html) — Audit logging pattern
- [Terminal Emulator Comparison 2026](https://blog.luminoid.dev/Terminal-Emulator-Comparison-2026) — Modern terminal landscape

---

## 21. Summary

This architecture design:

1. **Fulfills the brief**: Governance-first, event-sourced, local-first, model-swappable, offline-capable.
2. **Addresses all gaps**: MCP, ACP, terminal emulator fixes, worker isolation, manifest lifecycle, resource budgets, vision provider.
3. **Rejects the wrong things**: Electron, Wave clone, pure microkernel, cloud-first.
4. **Chooses the right things**: Rust, wgpu/iced, vte parser, Tokio, event sourcing, capability-based security.
5. **Provides a build order**: 10 phases, bottom-up, each with clear done conditions.
6. **Is honest about risks**: Terminal emulator is hard, model switching is slow, event store grows.
7. **Is testable**: Every layer has unit tests, integration tests, failure tests, acceptance criteria.

The next step is **Phase 0**: repository setup, CI, and safety rails.
