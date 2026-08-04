# NexusAOS — Implementation-Ready Architecture Plan

> **Status**: Finalized
> **Date**: 2026-08-04
> **Approach**: Bottom-up, dependency-ordered, test-first

---

## 1. Current State

**This is a completion and hardening project, not greenfield.** The workspace has 14 crates + 2 binaries with substantial implementation already in place.

### What Exists
- `nexusaos-kernel` — Kernel, scheduler, event store, policy, tools, CLI, TUI, resource, context, router, manifest, artifact, worker
- `nexusaos-rpc` — Unix socket JSON-RPC
- `nexusaos-terminal` — PTY manager + Zig VT100 parser FFI
- `nexusaos-tui` — ratatui TUI
- `nexusaos-gui` — iced GUI (0.14, tokio, canvas)
- `nexusaos-ai` — Provider abstraction (Anthropic, OpenAI, OpenAI-compatible, Qwen vision)
- `nexusaos-blockctl` — Shell controller, filestore
- `nexusaos-remote` — SSH client, remote shell
- `nexusaos-waveobj`, `nexusaos-wps`, `nexusaos-wconfig`, `nexusaos-vault` — Support crates
- `nexusaos-mcp`, `nexusaos-acp` — Shell stubs exist

### What Is Broken or Missing

| Gap | Severity | Location |
|---|---|---|
| Terminal ANSI parsing | **High** | `nexusaos-terminal/src/ffi.rs` Zig FFI; `nexusaos-gui/src/terminal.rs` manual parser bugs |
| RPC `id` type rejection | **High** | `nexusaos-rpc/src/message.rs` rejects integer IDs (JSON-RPC 2.0 violation) |
| Kernel `KernelConfig` refactor incomplete | **Medium** | `nexusaos-kernel/src/runtime/kernel.rs` mixed positional/config patterns |
| MCP/ACP integration incomplete | **Medium** | Crates exist but may not be fully wired |
| Worker isolation not implemented | **Medium** | `nexusaos-kernel/src/worker.rs` may be stub |
| Manifest lifecycle incomplete | **Medium** | `nexusaos-kernel/src/manifest.rs` states may be incomplete |
| Resource budgets not enforced | **Medium** | `nexusaos-kernel/src/resource.rs` may lack hard ceilings |
| Policy decisions not logged as events | **Low** | `EventKind` may lack `PolicyDecision` |
| Project state summaries missing | **Low** | `nexusaos-kernel/src/project_summary.rs` may be incomplete |
| Execution artifacts missing | **Low** | `nexusaos-kernel/src/artifact.rs` may be incomplete |

---

## 2. Architecture Decision Records

### ADR-001: Keep Existing Crate Structure

**Decision**: Do NOT reorganize crates. Fix and complete within existing structure.

**Rationale**: Reorganization risks breaking downstream dependencies. Existing crate names are descriptive and consistent. Tests and CI already target this structure.

### ADR-002: Replace Zig VT100 FFI with `vte` Crate

**Decision**: Replace `ZigVt100Parser` with `vte::Parser` + custom `Perform` implementation in pure Rust.

**Rationale**: Eliminates Zig/FFI complexity. Uses battle-tested ANSI state machine (Alacritty, WezTerm, Kitty). Already a workspace dependency in `nexusaos-gui`. Enables direct bug fixes without cross-language rebuild.

**Trade-off**: Small performance risk from Rust vs Zig. Mitigated by `vte`'s zero-allocation design.

### ADR-003: Fix RPC `id` to Accept Integer and String

**Decision**: Change `RpcId` to `#[serde(untagged)] enum RpcId { Num(i64), Str(String) }` and make the field `Option<RpcId>`.

**Rationale**: Conformance to JSON-RPC 2.0 spec. Fixes integration with standard MCP/ACP clients.

### ADR-004: Complete `KernelConfig` Refactor

**Decision**: Complete the `KernelConfig` refactor. Do NOT revert to positional args.

**Rationale**: Config struct is already partially in place. Positional args are unmaintainable for 13 parameters. All test files already expect `KernelConfig`.

### ADR-005: Enforce Hard Resource Budgets

**Decision**: Add `ResourceBudget` struct with hard ceilings. Kernel must refuse new tasks when budgets are exceeded. Emit `ResourceBudgetExceeded` event.

**Rationale**: 16 GB RAM / 6 GB VRAM is constrained. Soft budgets are meaningless without hard enforcement.

### ADR-006: Log Policy Decisions as Events

**Decision**: Add `EventKind::PolicyDecision` and `EventPayload::PolicyDecision`. Every `PolicyEngine::evaluate()` call must emit an event.

**Rationale**: Auditability is a core requirement. Policy decisions are state changes that must be replayable.

### ADR-007: Terminal GUI Rendering Backend

**Decision**: Use raw `wgpu` instanced rendering for the terminal grid in `nexusaos-gui`.

**Rationale**: The terminal is the primary UI surface. Input latency and scrollback performance are user-visible. Raw `wgpu` is necessary for 60fps rendering with per-character ANSI updates. `iced::widget::canvas` is simpler but insufficient for terminal performance requirements.

### ADR-008: GUI Direct Kernel Dependency

**Decision**: Add `nexusaos-kernel` as a direct dependency to `nexusaos-gui`.

**Rationale**: The GUI is a native desktop app on the same host. Direct access is simpler, matches the existing TUI pattern, and avoids inventing a separate IPC translation layer just for the GUI.

---

## 3. Build Order (Dependency-Graph Driven)

```
Phase 0 (Safety Rails)
    ↓
Phase 1 (Domain Types)
    ↓
Phase 2 (Event Store Hardening)
    ↓
Phase 3 (Kernel Runtime Completion)
    ↓
Phase 4 (Resource Budgets) ← Phase 3
    ↓
Phase 5 (Model Providers) ← Phase 3
    ↓
Phase 6 (Tool Broker Hardening) ← Phase 3, 5
    ↓
Phase 7 (Terminal Engine Fixes) ← Phase 1, 6
    ↓
Phase 8 (CLI/TUI/GUI Polish) ← Phase 3, 7
    ↓
Phase 9 (IPC/MCP/ACP) ← Phase 3, 6
    ↓
Phase 10 (Hardening) ← all above
```

---

## 4. Phase Details

### Phase 0 — Safety Rails

**Goal**: Establish baseline quality gates.

**Steps**:
1. Run `cargo test --workspace` — capture current pass/fail count
2. Run `cargo clippy --workspace -- -D warnings` — capture warnings
3. Run `cargo fmt --check` — capture formatting issues
4. Fix compilation errors that block testing
5. Add `cargo deny` for license/security auditing
6. Add `cargo udeps` for unused dependency detection

**Done when**: `cargo test --workspace` passes (or documents all failures), `cargo clippy` passes with 0 warnings, `cargo fmt --check` passes.

**Deliverables**: `tests/baseline_2026-08-04.txt` — test count, clippy warnings, formatting issues.

---

### Phase 1 — Domain Types Completion

**Goal**: Ensure all core types are correct, serializable, and tested.

**Files**: `nexusaos-kernel/src/{task,state,events,error,capability,manifest}.rs`

**Changes**:
1. `events.rs`: Add missing `EventKind` variants: `PolicyDecision`, `ResourceBudgetExceeded`, `ModelResponded`, `CheckpointCreated`
2. `state.rs`: Verify state machine transitions. Add `#[cfg(test)] mod tests` with `valid_transitions()` covering all allowed transitions.
3. `manifest.rs`: Add `ManifestState` variants: `Draft → Validated → Signed → Active → Superseded → Retired`. Add `validate()`, `sign()`, `supersede()` methods.
4. `error.rs`: Ensure `NexusError` implements `std::error::Error` with descriptive messages.

**Done when**: All types compile with `#![deny(warnings)]`, each type has serialization round-trip tests, `EventKind` has all required variants.

---

### Phase 2 — Event Store Hardening

**Goal**: Ensure event store is append-only, checksummed, and replayable.

**Files**: `nexusaos-kernel/src/storage/{event_store,sqlite_event_store,snapshot,projection}.rs`

**Changes**:
1. `event_store.rs`: Add `checksum: String` to `Event`. Add `verify_checksum() -> bool`.
2. `sqlite_event_store.rs`: Add `idempotency_key` column to prevent duplicate appends.
3. `snapshot.rs`: Add `Snapshot` struct with `id`, `task_id`, `state`, `events`, `created_at`.
4. `projection.rs`: Add `rebuild()` method that replays all events from a snapshot.

**Done when**: Events can be appended, read back, and checksum verified. Duplicate events are rejected. Snapshots can be saved and loaded. Projection rebuilds from snapshot + events.

---

### Phase 3 — Kernel Runtime Completion

**Goal**: Ensure kernel accepts tasks, enforces policy, and persists state transitions.

**Files**: `nexusaos-kernel/src/runtime/{kernel,scheduler,replay,shutdown}.rs`

**Changes**:
1. `kernel.rs`: Complete `KernelConfig` refactor — replace all positional `Kernel::new(...)` calls with `KernelConfig { ... }`. Remove `#[derive(Debug)]` from `KernelConfig`. Add `KernelConfig::default()`.
2. `kernel.rs`: Add policy decision logging — every `policy.evaluate()` must emit `EventKind::PolicyDecision` with payload containing `task_id`, `action`, `decision`, `reason`.
3. `kernel.rs`: Add resource budget enforcement — before `submit_task()`, call `resource_monitor.check_budget(&resource_budget)`. If exceeded, emit `EventKind::ResourceBudgetExceeded` and return error.
4. `scheduler.rs`: Add `queue_depth()` method and enforce `max_queue_depth` from `ResourceBudget`.
5. `replay.rs`: Add `replay_from(task_id, from_event)` for partial replay.

**Done when**: Kernel constructs via `KernelConfig`. Every policy check emits an event. Resource budget refusal emits an event. Scheduler enforces max queue depth.

---

### Phase 4 — Resource Budget Enforcement

**Goal**: Add hard ceilings for RAM, VRAM, context, queue, disk.

**Files**: `nexusaos-kernel/src/{resource,context}.rs`

**Changes**:
1. `resource.rs`: Add `ResourceBudget` struct with `max_ram_mb`, `max_vram_mb`, `max_context_tokens`, `max_queue_depth`, `min_disk_free_gb`. Add `SystemPressure` from `sysinfo`. Add `check_all(pressure, budget) -> Vec<String>`.
2. `context.rs`: Add `context_for_task(task_type: TaskType) -> usize` policy. Clamp to `resource_budget.max_context_tokens`.

**Done when**: ResourceMonitor reports accurate system pressure. Context manager selects context size based on task type. Kernel refuses tasks when budgets exceeded.

---

### Phase 5 — Model Provider Completion

**Goal**: Ensure all three target models are swappable and loadable.

**Files**: `nexusaos-kernel/src/model/{provider,registry,types,claude,openai_compat,qwen_vision}.rs`

**Changes**:
1. `provider.rs`: Ensure `ModelProvider` trait has `complete()`, `stream()`, `health_check()`, `capabilities()`, `warmup()`, `unload()` — all async, returning `Result<_, ProviderError>`.
2. `registry.rs`: Ensure `ProviderRegistry` has `register()`, `get()`, `health_check_all()`. Use `catch_unwind` in health checks.
3. `qwen_vision.rs`: Ensure vision provider accepts images (base64 or URL), returns structured `Observation`, implements `ModelRole::Vision`.
4. `openai_compat.rs`: Ensure it works for both Ollama and LM Studio.

**Done when**: All three models can be registered by role. Health checks don't crash kernel. Vision provider returns structured output.

---

### Phase 6 — Tool Broker Hardening

**Goal**: Ensure all tools go through policy and capability checks.

**Files**: `nexusaos-kernel/src/tools/{broker,executor,filesystem,git,terminal,search_fetch}.rs`

**Changes**:
1. `broker.rs`: Ensure `ToolBroker::execute()` calls `policy.evaluate()` before execution, checks `CapabilitySet`, emits `EventKind::ToolInvoked` before and `EventKind::ToolResult` after.
2. `filesystem.rs`: Add path validation — reject paths outside `allowed_paths`, reject paths in `denied_paths`, check file size.
3. `terminal.rs`: Add sandboxing — use `bwrap` if available, enforce timeout, check denied command prefixes.

**Done when**: Every tool call passes through policy. Every tool call emits events. Filesystem scope is enforced. Terminal execution is sandboxed.

---

### Phase 7 — Terminal Emulator Fixes

**Goal**: Make the terminal emulator actually work with correct ANSI parsing.

**Files**: `nexusaos-terminal/src/{pty,parser}.rs`, `nexusaos-gui/src/terminal.rs`, `nexusaos-rpc/src/message.rs`, `nexusaos-blockctl/src/shell.rs`

**Changes**:
1. **Remove Zig FFI**: Delete `nexusaos-terminal/src/ffi.rs` and `build.rs`. Remove Zig build dependency.
2. **Add `vte` to `nexusaos-terminal`**: Add `vte = "0.13"` to `Cargo.toml`.
3. **Create `parser.rs`**: Implement `vte::Perform` trait for terminal parser with grid state, scrollback, cursor.
4. **Fix PTY input**: `Ctrl+C` → `(c.to_ascii_lowercase() as u8) & 0x1F`. `Enter` → send `\r` (0x0D). Use `VecDeque` for scrollback.
5. **Fix RPC `id`**: `#[serde(untagged)] enum RpcId { Num(i64), Str(String) }` with `Option<RpcId>`.
6. **Fix `stop()` race**: Use `AtomicU8` with `compare_exchange` for status (`Idle(0)`, `Running(1)`, `Stopping(2)`, `Stopped(3)`).
7. **GUI terminal rendering**: Implement raw `wgpu` instanced rendering in `nexusaos-gui/src/terminal.rs` for the terminal grid block.

**Done when**: VT100/ANSI test vectors pass. Ctrl+C works for uppercase. Enter sends `\r`. RPC accepts both integer and string IDs. `stop()` is race-free. Input latency < 16ms. Terminal GUI block renders at 60fps via raw `wgpu`.

---

### Phase 8 — CLI/TUI/GUI Polish

**Goal**: Make the system inspectable and usable.

**Files**: `bin/nexusaos-cli/src/main.rs`, `nexusaos-kernel/src/cli/*.rs`, `nexusaos-tui/src/*.rs`, `nexusaos-gui/src/*.rs`, `nexusaos-gui/Cargo.toml`

**Changes**:
1. **CLI**: Ensure each command maps to one kernel action: `init`, `doctor`, `status`, `plan <task>`, `run <task>`, `replay <task-id>`, `tools list`, `models list`.
2. **TUI**: Wire `nexusaos-tui` to kernel — display task state, tool results, model responses, resource pressure.
3. **GUI dependency**: Add `nexusaos-kernel` as a direct dependency in `nexusaos-gui/Cargo.toml`.
4. **GUI**: Wire `nexusaos-gui` to kernel directly — display terminal blocks via `nexusaos-blockctl`, model indicator, resource gauges.

**Done when**: All CLI commands work end-to-end. TUI displays kernel state. GUI displays terminal blocks and task state.

---

### Phase 9 — IPC/MCP/ACP Completion

**Goal**: External control and standardized tool access.

**Files**: `nexusaos-rpc/src/*.rs`, `nexusaos-mcp/src/*.rs`, `nexusaos-acp/src/*.rs`

**Changes**:
1. **RPC**: Wire `RpcHandler::handle()` to `Kernel::submit_task()` / `Kernel::execute_task()`. All requests pass through policy engine.
2. **MCP**: Complete `McpServer::handle_request()` routing to `ToolBroker::execute()`. All requests pass through `PolicyEngine::evaluate()` and `CapabilitySet` checks. Emit `EventKind::PolicyDecision`.
3. **ACP**: Complete `AcpServer::handle_session()` with capability lease scoped to session. All requests pass through kernel.

**Done when**: JSON-RPC works over Unix socket. MCP tools pass through policy and emit events. ACP clients receive explicit capabilities.

---

### Phase 10 — Hardening and Release Readiness

**Goal**: Safe enough to trust.

**Steps**:
1. Run `cargo test --workspace` and `cargo test --workspace --benches`
2. Run VT100 test vectors from `vte` crate. Verify ANSI colors, cursor positioning, scrollback, mouse reporting. Measure input latency < 16ms.
3. Security audit: verify no tool bypass, no model bypass, no unlogged state changes, no silent writes, secret redaction works.
4. Performance profiling: kernel task submission latency, tool execution latency, terminal rendering frame time, memory usage (kernel < 20 MB).
5. Documentation: `README.md`, `docs/architecture.md`, `docs/contributing.md`, inline docs for all public APIs.

**Done when**: All tests pass, 0 clippy warnings, terminal passes VT100 compliance, no security bypasses found, memory usage within budget.

---

## 5. Terminal Emulator Decision Matrix

| Option | Pros | Cons | Decision |
|---|---|---|---|
| Fix existing Zig FFI parser | Minimal code change | Zig dependency, unsafe FFI, hard to debug | **Rejected** |
| Replace with `vte` crate (chosen) | Battle-tested, pure Rust, zero-alloc | Need to implement `Perform` trait | **Accepted** |
| Use `vt100` crate instead | Higher-level API, includes screen | Larger dependency, less control | **Deferred** |
| Drop terminal emulator entirely | Simplifies scope | Breaks brief requirement | **Rejected** |

**GUI rendering**: Raw `wgpu` instanced rendering for terminal grid (not `iced::widget::canvas`). Rationale: terminal is primary UI surface; 60fps scrollback requires per-character GPU instancing.

---

## 6. Validation Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] Terminal passes VT100/ANSI test vectors
- [ ] Input latency < 16ms
- [ ] No tool bypass (all calls go through `ToolBroker`)
- [ ] No model bypass (all calls go through `Kernel`)
- [ ] No unlogged state changes (every transition emits `Event`)
- [ ] No silent writes (all writes go through `EventStore`)
- [ ] Secret redaction verified
- [ ] Memory usage: kernel < 20 MB

---

## 7. Key Files Reference

| Component | Primary File(s) |
|---|---|
| Kernel | `crates/nexusaos-kernel/src/runtime/kernel.rs` |
| Scheduler | `crates/nexusaos-kernel/src/runtime/scheduler.rs` |
| Event store | `crates/nexusaos-kernel/src/storage/event_store.rs` |
| SQLite store | `crates/nexusaos-kernel/src/storage/sqlite_event_store.rs` |
| Snapshot | `crates/nexusaos-kernel/src/storage/snapshot.rs` |
| Projection | `crates/nexusaos-kernel/src/storage/projection.rs` |
| Event types | `crates/nexusaos-kernel/src/events.rs` |
| Task types | `crates/nexusaos-kernel/src/task.rs` |
| State machine | `crates/nexusaos-kernel/src/state.rs` |
| Policy engine | `crates/nexusaos-kernel/src/policy.rs` |
| Resource monitor | `crates/nexusaos-kernel/src/resource.rs` |
| Context manager | `crates/nexusaos-kernel/src/context.rs` |
| Tool broker | `crates/nexusaos-kernel/src/tools/broker.rs` |
| Model providers | `crates/nexusaos-kernel/src/model/{provider,registry,types,claude,openai_compat,qwen_vision}.rs` |
| PTY manager | `crates/nexusaos-terminal/src/pty.rs` |
| RPC messages | `crates/nexusaos-rpc/src/message.rs` |
| TUI app | `crates/nexusaos-tui/src/app.rs` |
| GUI terminal | `crates/nexusaos-gui/src/terminal.rs` |
| CLI main | `bin/nexusaos-cli/src/main.rs` |
