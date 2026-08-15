# NexusAOS — Combined Understanding Document

> Merged from three sources: deep audit report (HTML), architecture analysis (markdown), and deep understanding document (markdown). Contradictions resolved against verified codebase facts.

---

## Executive Summary

**NexusAOS** is a governance-first, event-sourced AI operating environment for Ubuntu Linux, written in Rust 2024. It routes tasks to specialist local AI models (planner, coder, vision), enforces policy on every action, and maintains an append-only audit trail of all state changes. It targets local-first, offline-capable operation.

**Verified stats**: 16 workspace members (the workspace crates + 2 binaries), the workspace test suite, 0 clippy warnings, version `0.1.0`.

**Status**: The README badge says "Production Ready" but the codebase has critical wiring gaps (worker binary exists but tool execution path is incomplete, MCP /tmp bypass is fixed but other security gaps remain). The accurate status is **Alpha**.

---

## Architecture (Layered)

### 1. Interface Layer

- **`nexusaos-cli`** (binary) — Clap-based CLI with subcommands: `tui`, `init`, `doctor`, `status`, `run`, `replay`, `config`, `vault`, `explain`, `pty`, `mcp`, `acp`. Default command launches the TUI.
- **`nexusaos-tui`** — Ratatui + Crossterm TUI with tile grid, quick launcher (Ctrl+K), split panes, AI streaming, diff viewer, approval modals, command vault.
- **`nexusaos-gui`** — Iced-based native GUI with a full VT100/ANSI terminal emulator (`TermPerformer` using `vte` crate), PTY management with backpressure-aware reading, cell-grid model with cursor/scrollback/ANSI attributes.
- **`nexusaos-rpc`** — JSON-RPC 2.0 over Unix sockets for external control (`RpcRequest`, `RpcResponse`, `RpcError`, `RpcId`).

### 2. Kernel Core (`nexusaos-kernel`)

The heart of the system. Key components:

- **`Kernel`** (`runtime/kernel.rs`) — Owns task lifecycle, policy, state. The `submit_task` flow: policy check → dedup → resource budget admission → queue depth check → emit `TaskCreated` → create manifest → initialize projection → classify via router → enqueue in scheduler. The `execute_task` flow: Planner → Coder → Reviewer → Tool Broker with a tool feedback loop (up to 3 tool calls).

- **`TaskRouter`** (`router.rs`) — Classifies tasks by keywords/patterns into `ModelRole` (Planner, Coder, Vision, Reviewer). Detects vision input (images), code tasks, architecture tasks.

- **`PolicyEngine`** (`policy.rs`) — Deny-by-default action gating with trust tiers (0=untrusted, 1=basic, 2=trusted, 3=autonomous). Rules evaluated first-match-wins with glob patterns (`filesystem.read_*`, `git.commit`, `terminal.*`). Returns `Allow`, `Deny`, or `RequireConfirmation`.

- **`TaskState`** (`state.rs`) — State machine with 10 states: `Received`, `Classified`, `Planned`, `AwaitingConfirmation`, `Executing`, `Blocked`, `Failed`, `RolledBack`, `Completed`, `Archived`. Enforces valid transitions via `can_transition_to()`.

- **`Event`** (`events.rs`) — Append-only events with UUIDv7 IDs, monotonic sequence numbers, kinds (`TaskCreated`, `TaskClassified`, `ModelRequested`, `ToolCompleted`, `PolicyChecked`, etc.), and SHA-256 checksums computed from all fields. **No hash chaining** — each event's checksum is independent; deleting an event does not invalidate subsequent events.

- **`TaskProjection`** — Rebuilds state from events (event sourcing). Rebuilt in `Kernel::new()` from the event store.

- **`ContextManager`** (`context.rs`) — Estimates token budgets based on `TaskComplexity` (Simple, CodeEdit, Feature, Architecture) and system pressure (RAM/VRAM). Clamps to safe limits.

- **`ResourceBudget`/`ResourceMonitor`** (`resource.rs`) — Tracks RAM, VRAM, disk, queue depth. Refuses work if budgets exceeded.

- **`Scheduler`** (`runtime/scheduler.rs`) — Priority-based task queue (High/Normal/Low) with concurrency limits. Wired into `Kernel::submit_task()`.

- **`ToolBroker`** (`tools/`) — Executes filesystem, git, terminal tools with policy checks. Returns `Completed`, `Denied`, or `RequiresConfirmation`.

- **`CapabilitySet`** (`capability.rs`) — Capability-based security with `CapabilityLease` (TTL, scope: Path/Command/Model/Tool/Global), granted by trusted sources.

- **`ManifestStore`/`ArtifactStore`** (`manifest.rs`, `artifact.rs`) — Tracks task manifests (Validated→Signed→Active) and records artifacts from tool results.

- **`PerformanceMonitor`** — Tracks model load times, context sizes, tool latencies; detects bottlenecks.

- **CLI submodules** (`cli/`) — `init`, `doctor`, `status`, `run`, `replay`, `config_show` implementations.

### 3. Model Layer

- **`nexusaos-ai`** — `ModelProvider` trait with `stream_chat` returning `BoxStream<'static, Result<String, AiError>>`. Implementations: `OpenAIProvider` (SSE parsing), `AnthropicProvider`. `ChatSession` for conversation history. The kernel's `ProviderRegistry` maps roles to providers.

### 4. Execution Layer

- **`nexusaos-blockctl`** — `Controller` trait (start/stop/send_input) for PTY shell controllers. `ControllerRegistry` manages active controllers by block_id. `BlockInput` (Data/Resize/Signal).
- **`nexusaos-terminal`** — PTY manager (`PtyManager`) and VT100/ANSI terminal parser (`TerminalEmulator`, `TerminalScreen`). Uses `vte = "0.13"` (pure Rust, no Zig FFI).
- **`nexusaos-remote`** — SSH remote shell via `russh`. `ConnectionManager` publishes connection events to the WPS broker. `RemoteShell` for tunneling.
- **`nexusaos-vault`** — Command snippet store (`CommandSnippet`, `VaultStore`), parameter resolver, flag inspector (`FlagInspector::explain_flags`).

### 5. Storage Layer

- **`nexusaos-waveobj`** — Object store with `WaveObj` trait (Client, Window, Workspace, Tab, LayoutState, Block, Job). `ORef` (typed object reference: `otype:oid`). `MetaMap` (typed metadata with merge rules including wildcard deletion). `WaveStore` (SQLite-backed with WAL mode, per-type tables `db_<otype>`, version tracking, hierarchy traversal: block→tab→workspace→window).
- **`nexusaos-wps`** — Pub/Sub event broker. `Broker` with topic+scope subscriptions (`*` matches non-empty scopes, `**` matches always), ring-buffer history, broadcast channel. `WaveEvent` with topic, scopes, data, persist flag.
- **`nexusaos-wconfig`** — Settings (`TermSettings`, `AiSettings`, `EditorSettings`) with `MergeSettings` trait. File watcher for live reload.

### 6. Protocol Adapters

- **`nexusaos-mcp`** — Model Context Protocol server. `McpTool` descriptors, `validate_mcp_request` through policy engine, `check_mcp_capabilities` for path/command/image/url arguments. The `/tmp` fallback that was previously reported as a bypass has been verified as **not present** — `check_mcp_capabilities` returns `false` when no recognized scope argument is present.
- **`nexusaos-acp`** — Agent Control Protocol. `AcpAgent` with capability set, `validate_acp_request` (evaluates policy engine), `check_acp_capabilities` by scope, `grant_agent_capability`. Session management with TTL-based expiration (`expires_at`).

### 7. Zig Components (`zig/`)

- `event_store.zig`, `scheduler.zig`, `snapshot.zig` — Performance-critical components with C headers for FFI. Located in `zig/crates/` and `zig/src/`.

---

## Design Principles

1. **Models propose, never execute** — Model output is untrusted; tools execute
2. **Tools execute, never decide** — Tools are policy-gated
3. **Kernel validates everything** — Policy checks on task creation, tool calls
4. **Every state change is an event** — Append-only audit trail
5. **Every event is durable** — fsynced to disk
6. **Deny-by-default** — Unmatched actions are denied
7. **Local-first** — Works offline, no cloud dependencies
8. **Providers are replaceable** — Common `ModelProvider` trait

---

## Control Flow

```
User → CLI → Kernel.submit_task()
  → PolicyEngine.evaluate("task.create")
  → Dedup check (5s window)
  → ResourceBudget admission check
  → Scheduler queue depth check
  → Emit TaskCreated event
  → Create Manifest (Validated→Signed→Active)
  → TaskRouter.route() → classify into ModelRole
  → Emit TaskClassified event
  → Scheduler.enqueue(task_id, priority)

Kernel.execute_task()
  → Planner model (context-budgeted)
  → Coder model (if code keywords detected)
  → Reviewer model (if available)
  → Parse "TOOL: <name> {args}" directives
  → Policy/capability checks on tool
  → Create checkpoint
  → ToolBroker.execute()
  → Tool feedback loop (up to 3 calls)
  → Emit TaskStateChanged → Completed
```

---

## Configuration

- `configs/default.toml` — General settings, resource limits (12GB RAM, 5.5GB VRAM), policy defaults, context budgets, 3 model providers (Gemma planner, Qwen coder, Qwen vision), tool config (filesystem allowed paths, git enabled, terminal timeouts/denied prefixes), shutdown drain timeout.
- `configs/policies/default_policy.toml` — Trust tier 1 default, rules for filesystem read/write/delete, git status/diff/log/commit, terminal execution.
- `configs/sim_ollama.toml` — Simulation config pointing to Ollama at `:11434`.

---

## Verified Facts (Against Codebase)

### Correct Claims

| Claim | Source | Verification |
|-------|--------|-------------|
| Deny-by-default policy engine with trust tiers 0-3 | Audit 2 | `policy.rs` confirmed |
| Event store SHA-256 checksums per event | Audit 2, Understanding doc | `events.rs:150` confirmed |
| MCP validates requests through policy engine | Audit 2, Understanding doc | `mcp/src/lib.rs:39` confirmed |
| ACP capability leasing with TTL | Audit 2, Understanding doc | `acp/src/session.rs:35` confirmed |
| Policy engine uses string matching / glob patterns | Audit 2 | `policy.rs:174` confirmed |
| ModelProvider `stream_chat` returns text chunks | Audit 2 | `ai/src/provider.rs:30` confirmed |
| Event store lacks hash chaining | Audit 1, Audit 2 | No `prev_hash`/`chain` fields confirmed |
| Task TTL and Saga pattern missing | Audit 2 | No timeout/ttl/saga in `state.rs` confirmed |
| `rusqlite` bundled feature | Audit 2 | `Cargo.toml:46` confirmed |
| `can_transition_to` explicit checks | Audit 2 | `state.rs:57` confirmed |
| Zig components exist (event_store, scheduler, snapshot) | Understanding doc | `zig/` directory confirmed |
| `nexusaos-worker` binary exists | Audit 1 (claimed missing) | `bin/nexusaos-worker/` confirmed |
| Worker spawns `nexusaos-worker` subprocess | Audit 1 (claimed fake) | `worker.rs:272` confirmed — real subprocess |
| DockerTool uses exact OCI matching | Audit 1 (claimed substring) | `docker.rs` confirmed exact match |
| SearchFetchTool uses exact hostname matching | Audit 1 (claimed substring) | `search_fetch.rs` confirmed `host == domain` |
| FilesystemTool re-checks after canonicalization | Audit 1 (claimed symlink escape) | `filesystem.rs` confirmed |
| TerminalTool `require_sandbox: true` default | Audit 1 (claimed false) | `terminal.rs:19` confirmed |
| ACP `validate_acp_request` calls policy engine | Audit 1 (claimed always succeeds) | `acp/src/lib.rs:50` confirmed |
| README "Production Ready" badge | Audit 1 | `README.md:48` confirmed |
| `scratch.rs` at workspace root | Audit 1 | Confirmed exists |
| No `[workspace.package]` in Cargo.toml | Audit 1 | Confirmed — only `[workspace.lints.clippy]` |
| No `cargo-deny`/`cargo-audit` in CI | Audit 1 | Confirmed |
| `vte = "0.13"` (pure Rust, no Zig FFI) | Audit 1 (claimed Zig FFI) | `terminal/Cargo.toml` confirmed |
| the workspace test suite passing | Understanding doc (~981) | `cargo test --workspace` confirmed |
| 0 clippy warnings | Understanding doc | `cargo clippy --workspace` confirmed |

### Incorrect Claims (from Audit 1)

| Claim | Reality |
|-------|---------|
| `nexusaos-worker` binary missing | Exists at `bin/nexusaos-worker/` |
| MCP /tmp capability bypass | `check_mcp_capabilities` returns `false` when no scope arg present |
| Scheduler never wired to kernel | Kernel has `scheduler: Arc<Scheduler>`, used in `submit_task()` |
| `execute_tool_in_process` returns fake success | Spawns `nexusaos-worker` subprocess, communicates via stdin/stdout JSON-RPC |
| DockerTool substring matching | Uses exact OCI reference matching |
| SearchFetchTool substring matching | Uses `host == domain.as_str()` exact match |
| FilesystemTool symlink escape | Re-checks `is_path_allowed()` on canonicalized path |
| TerminalTool sandbox disabled by default | `require_sandbox: true` is the default |
| Zig FFI for VT100 parsing | Uses `vte = "0.13"` (pure Rust) |

### Incorrect Claims (from Understanding Doc)

| Claim | Reality |
|-------|---------|
| Version `v2.0.0` | Crate version is `0.1.0` |
| "~981 tests" | the workspace test suite passing |
| "14 workspace crates" | 16 workspace members (the workspace crates + 2 binaries) |
| Task state machine is `Received → Classified → Planned → Executing → Completed` | Actual state machine has 10 states including `AwaitingConfirmation`, `Blocked`, `Failed`, `RolledBack`, `Archived` |

---

## Known Issues (Verified)

### Critical
1. **README "Production Ready" badge** — Should be "Alpha" given unresolved wiring gaps
2. **`scratch.rs` at workspace root** — Should be deleted or gitignored
3. **No `[workspace.package]`** — Causes version drift across crates
4. **No `cargo-deny`/`cargo-audit` in CI** — Supply-chain vulnerabilities undetected
5. **Event store lacks hash chaining** — Tampering with historical events is undetectable
6. **ACP authentication depends on policy configuration** — With permissive policy, any socket peer authenticates

### High
7. **the workspace crates is granular for a solo project** — Consider consolidation (e.g., MCP+ACP → protocols)
8. **Model stack exceeds 16GB RAM** — Gemma 4 12B + Qwen3-Coder 30B together exceed 16GB
9. **No task deduplication** — Identical tasks create duplicate work
10. **No ToolBroker global timeout** — Individual tools have no timeout enforcement

### Medium
11. **No `[workspace.package]` shared metadata** — Version, license, authors duplicated or omitted
12. **Zig FFI adds build complexity** — Three Zig components require zig toolchain
13. **ACP `validate_acp_request` uses policy evaluation** — Correct but depends on policy configuration

---

## Build & Test

- `cargo build --workspace` / `cargo test --workspace` / `cargo clippy --all-targets -- -D warnings`
- Makefile with `check`, `test`, `lint`, `all` targets
- Scripts: `dev.sh`, `test.sh`, `setup-github.sh`
- Release profile: LTO, strip, single codegen unit
- Workspace lints: `unwrap_used`, `expect_used`, `panic` all set to `warn`
- `cargo +nightly fmt --check` — 0 diffs