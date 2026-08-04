# NexusAOS v1 Architecture Brief — Implementation Plan

## Goal

Implement the core NexusAOS v1 architecture as described in `nexus_aos_architecture_brief.md` sections 1–15, without the v2 refinements in section 16 (worker isolation, resource budgets, manifest lifecycle, task lifecycle formalization, replay semantics, heavy model policy, tool sandboxing).

## Current State (Verified Against Codebase)

The following v1 components are **already implemented**:

| Component | Location | Status |
|---|---|---|
| ModelProvider trait + OpenAI/Anthropic providers | `model/provider.rs`, `model/openai_compat.rs`, `model/claude.rs` | Done |
| ModelRole enum (Planner, Coder, Vision, Reviewer) | `state.rs` | Done |
| ModelProviderConfig with role field and `into_provider()` | `config.rs:124` | Done |
| ProviderRegistry (register, get, available_roles, health_check_all) | `model/registry.rs` | Done |
| TaskRouter with keyword-based classification | `router.rs` | Done |
| RouteDecision with primary_role, review_role, confidence | `router.rs:12` | Done |
| PolicyEngine with evaluate(), rules, trust tiers | `policy.rs` | Done |
| PolicyDecision (Allow, Deny, RequireConfirmation) | `policy.rs:12` | Done |
| TrustTier (Untrusted, Low, Medium, High) | `policy.rs:63` | Done |
| ReplayEngine with event-log replay | `runtime/replay.rs` | Done |
| ResourceMonitor with SystemPressure snapshot | `resource.rs:124` | Done |
| JsonlEventStore + SqliteEventStore | `storage/event_store.rs`, `storage/sqlite_event_store.rs` | Done |
| SnapshotStore | `storage/snapshot.rs` | Done |
| TaskProjection (state reconstruction from events) | `storage/projection.rs` | Done |
| ToolBroker (register, execute, available_tools) | `tools/broker.rs` | Done |
| ToolExecutor trait | `tools/executor.rs` | Done |
| FilesystemTool, GitTool, TerminalTool | `tools/filesystem.rs`, `tools/git.rs`, `tools/terminal.rs` | Done |
| Kernel (submit_task, execute_task, transition_task) | `runtime/kernel.rs` | Done |
| Task state machine (Received→Classified→Planned→...→Archived) | `state.rs` | Done |
| EventStore (append-only JSONL) | `storage/event_store.rs` | Done |
| EventKind + EventPayload (core variants) | `events.rs` | Done (partial — v2 added more) |
| CLI (nexusaos-cli binary) | `bin/nexusaos-cli/` | Done |

The following v1 components are **NOT yet implemented**:

| Gap | Brief Reference | Severity |
|---|---|---|
| Search/Fetch tool | §6.6 (Tool Layer) | Medium |
| Docker tool | §6.6 (Tool Layer) | Medium |
| Vision provider producing structured observations (not direct actions) | §6.8 | High |
| Planner constraint: no direct file writes without delegation | §6.9 | Medium |
| Coder constraint: no scope/architecture decisions without planner input | §6.10 | Medium |
| Checkpoint triggers (before file writes, git commits, destructive actions) | §6.7 | High |
| Failure mode handling (model retry, fallback, degradation) | §9 | High |
| System failure recovery (startup scan, replay, checkpoint restoration) | §9 | High |
| User-caused edge case handling (clarification, confirmation, dedup, cancellation) | §9 | Medium |
| Secret redaction in audit logs | §10 | Medium |
| Model switching strategy (one at a time, not concurrent) | §11 | Medium |
| Summary caching (not huge raw histories) | §11 | Low |
| Bottleneck detection (model loading, context growth, tool latency) | §11 | Low |
| Acceptance criteria tests (traceable, reversible, no silent destructive, recovery, consistent routing) | §12 | High |

## What v1 Does NOT Include (Deferred to v2)

- MCP Integration (§6.11) — added in v2
- ACP Integration (§6.11) — added in v2
- Qwen3.5 9B Vision Provider — added in v2
- Worker Isolation (same-machine isolated workers) — added in v2
- Manifest Lifecycle (draft→validated→signed→active→superseded→retired) — added in v2
- Resource Budgets with hard ceilings — added in v2
- Project State Summaries — added in v2
- Execution Artifacts — added in v2
- Policy Decisions as Events — added in v2
- Task lifecycle formalization (received→classified→planned→authorized→executing→...) — added in v2
- Replay semantics (control-plane replay, not token regeneration) — added in v2
- Heavy model policy (30B coder cold-loaded with hard queueing) — added in v2
- Tool sandboxing (same-machine isolated workers with capability leases) — added in v2

## Gaps to Fill for v1

### 1. Search/Fetch Tool

**Brief reference**: §6.6 (Tool Layer)

**What's needed**:
- Implement a Search/Fetch tool with permission gating
- Tool must pass through PolicyEngine before execution
- Tool results must be logged as events
- Scope: only fetch from permitted URLs/domains

**Dependencies**: `ToolBroker`, `ToolExecutor` trait, `PolicyEngine`

### 2. Docker Tool

**Brief reference**: §6.6 (Tool Layer)

**What's needed**:
- Implement a Docker tool for container actions
- Tool must pass through PolicyEngine before execution
- Tool results must be logged as events
- Scope: only permitted container operations

**Dependencies**: `ToolBroker`, `ToolExecutor` trait, `PolicyEngine`

### 3. Vision Provider — Structured Observations Only

**Brief reference**: §6.8 (Vision Specialist)

**What's needed**:
- Vision provider must produce structured observations, not direct actions
- Constraint: vision model should not directly control system actions
- Vision outputs must be converted to structured observations before use
- Note: Qwen3.5 9B integration is deferred to v2; v1 can use a stub or OpenAI-compatible vision provider

**Dependencies**: `ModelProvider` trait, `ModelRole::Vision`

### 4. Planner Constraint Enforcement

**Brief reference**: §6.9 (Planner / Architect Specialist)

**What's needed**:
- Planner must not write files directly unless explicitly delegated
- Enforce this constraint in the tool execution path when the assigned role is Planner
- Add a policy rule or capability check that prevents Planner from direct file writes

**Dependencies**: `PolicyEngine`, `ToolBroker`, `ModelRole::Planner`

### 5. Coder Constraint Enforcement

**Brief reference**: §6.10 (Coder Specialist)

**What's needed**:
- Coder must not decide product scope or architecture direction without planner input for large tasks
- Enforce this constraint in the routing/policy layer
- Add a policy rule that requires planner review for architecture-level changes initiated by Coder

**Dependencies**: `PolicyEngine`, `TaskRouter`, `ModelRole::Coder`

### 6. Checkpoint Triggers

**Brief reference**: §6.7 (Replay and Checkpoint System)

**What's needed**:
- Add checkpoint triggers: before file writes, git commits, package installs, destructive actions, long multi-step tasks
- Store snapshots before risky actions using `SnapshotStore`
- Integrate checkpoint creation into `ToolBroker::execute()` and `Kernel::execute_task()`

**Dependencies**: `SnapshotStore`, `EventStore`, `ToolBroker`, `Kernel`

### 7. Failure Mode Handling

**Brief reference**: §9 (Failure Modes and Edge Cases)

**What's needed**:
- Model failure: retry with smaller context, route to fallback model, ask planner for re-scope, degrade to partial result
- Tool failure: preflight checks, staged execution, rollback checkpoints, error classification
- System failure: event sourcing recovery, startup scan, replay from event log, checkpoint restoration
- User-caused edge cases: clarification prompts, confirmation gates, task deduplication, cancellation support

**Dependencies**: `EventStore`, `ReplayEngine`, `SnapshotStore`, `ProviderRegistry`, `PolicyEngine`

### 8. Security Hardening

**Brief reference**: §10 (Security and Privacy)

**What's needed**:
- Enforce trust boundaries (user input untrusted, model output untrusted, tool results partially trusted)
- Capability-based filesystem scoping (already in `CapabilitySet::check_path`)
- Shell access sandboxed or heavily constrained (already in `TerminalTool` denied_prefixes)
- Destructive operation confirmation gates (already in `PolicyDecision::RequireConfirmation`)
- Secret redaction in audit logs — NOT yet implemented
- Local-first by default, no cloud telemetry — partially implemented

**Dependencies**: `PolicyEngine`, `CapabilitySet`, `EventStore`, `TerminalTool`

### 9. Performance Strategy

**Brief reference**: §11 (Performance and Resource Analysis)

**What's needed**:
- Model loading strategy: one specialist at a time when possible (not concurrent residency)
- Summary caching: cache summaries, not huge raw histories
- Bottleneck detection: model loading time, context growth, tool invocation latency
- Integrate with existing `ResourceMonitor::snapshot()` for basic tracking

**Dependencies**: `ProviderRegistry`, `ResourceMonitor`, `EventStore`

### 10. Testing and Validation

**Brief reference**: §12 (Testing and Validation Plan)

**What's needed**:
- Unit tests: task classification, policy decisions, state transitions, event serialization, rollback logic, provider abstraction
- Integration tests: model call to tool call loop, filesystem write and rollback, git commit and revert
- Failure tests: backend unavailable, tool returns error, write permission denied, disk full, cancellation mid-task, task replay after crash
- Acceptance criteria: tasks are traceable, actions are reversible, no silent destructive actions, system recovers from restarts, router chooses specialists consistently

**Dependencies**: All components above

## Execution Order

1. **Checkpoint Triggers** (foundation — needed for failure recovery and rollback)
2. **Failure Mode Handling** (resilience — depends on checkpoints)
3. **Search/Fetch Tool** (tool layer completion)
4. **Docker Tool** (tool layer completion)
5. **Vision Provider — Structured Observations** (specialist model)
6. **Planner Constraint Enforcement** (governance)
7. **Coder Constraint Enforcement** (governance)
8. **Security Hardening** (trust boundaries, secret redaction)
9. **Performance Strategy** (model switching, caching, bottleneck detection)
10. **Testing and Validation** (verify everything works)

## Validation

- All existing tests continue to pass
- New tests cover each gap per §12 acceptance criteria
- 0 clippy warnings
- Each new component follows existing patterns (edition 2021, async-trait, proper error types)
- MCP/ACP are NOT included in v1 (deferred to v2)
- Worker isolation, resource budgets, manifest lifecycle are NOT included in v1 (deferred to v2)

## Open Questions

1. Should the Search/Fetch tool support both local search and remote fetch, or just remote fetch?
2. Should the Docker tool support full container lifecycle or just run/stop?
3. What is the fallback model when the primary model fails?
4. How should checkpoint frequency be configured (every tool call, only for destructive actions, or configurable)?
5. Should secret redaction be a general-purpose utility or specific to the event store?
6. What constitutes a "large task" that requires planner input for the coder constraint?
7. Should the vision provider in v1 be a stub or a functional OpenAI-compatible vision provider?
