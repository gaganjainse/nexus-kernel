# NexusAOS v2 Architecture Brief — Completion Plan

## Goal

Close the remaining gaps between the current nexus-kernel implementation and the NexusAOS v2 Architecture Brief (`nexus_aos_architecture_brief.md`).

## Current State

~85% of the brief is implemented. The following components are missing or incomplete:

| Gap | Severity | Brief Section |
|---|---|---|
| MCP Integration | High | §6.11 |
| ACP Integration | High | §6.11 |
| Vision Provider (Qwen3.5 9B) | High | §3, §6.3 |
| Worker Isolation | Medium | §5.6, §16 refinement |
| Manifest Lifecycle | Medium | §16 refinement |
| Resource Budgets (hard ceilings) | Medium | §16 refinement |
| Project State Summaries | Low | §6.5 |
| Execution Artifacts | Low | §6.5 |
| Policy Decisions as Events | Low | §7 control flow |

## Tasks

### 1. MCP Integration

**Brief reference**: §6.11 — MCP extends tool access; all MCP requests must pass through Policy Engine, Capability Manager, and Kernel.

**Implementation**:
- Add `crates/nexusaos-mcp/` crate with MCP protocol support
- Implement MCP tool adapter that wraps existing `ToolExecutor` implementations
- Add MCP request validation through `PolicyEngine::evaluate()` before tool execution
- Add MCP capability check through `CapabilitySet::check_path()` / `check_command()`
- Expose MCP endpoints via `nexusaos-rpc` or a new MCP server

**Dependencies**: Tool Broker, Policy Engine, Capability System (all exist)

### 2. ACP Integration

**Brief reference**: §6.11 — ACP extends agent/client interaction; ACP-connected agents must receive explicit capabilities before acting.

**Implementation**:
- Add `crates/nexusaos-acp/` crate with ACP protocol support
- Implement ACP session management with capability granting
- Add ACP client authentication and capability verification
- Ensure ACP requests flow through Kernel (not bypass governance)
- Integrate with existing `CapabilitySet` for permission checks

**Dependencies**: Capability System, Kernel, RPC (all exist)

### 3. Vision Provider (Qwen3.5 9B)

**Brief reference**: §3 (assumptions), §6.8 (Vision Specialist)

**Implementation**:
- Add vision provider configuration to `ModelProviderConfig` in `config.rs`
- Implement or integrate a Qwen3.5 9B vision-capable provider
- Add `ModelRole::Vision` provider registration in `model/registry.rs`
- Ensure vision outputs are converted to structured observations (§6.8 constraint)
- Add vision capability to `CapabilitySet` scope

**Dependencies**: Model Provider trait, Registry (exist)

### 4. Worker Isolation

**Brief reference**: §16 refinement — tools should run as same-machine isolated workers with explicit capability leases

**Implementation**:
- Refactor `ToolExecutor` trait to support isolated worker processes
- Add worker process management (spawn, monitor, terminate)
- Implement capability lease passing to workers
- Add worker health monitoring and restart logic
- Keep same-machine local execution (not networked microservices)

**Dependencies**: Tool Executor, Capability System (exist)

### 5. Manifest Lifecycle

**Brief reference**: §16 refinement — `draft → validated → signed → active → superseded → retired`

**Implementation**:
- Add `Manifest` struct with lifecycle states
- Implement state transition validation
- Add manifest persistence (event store or separate store)
- Add manifest validation logic (schema, policy compliance)
- Add manifest signing/verification mechanism
- Ensure manifests are immutable once active

**Dependencies**: Event Store, Policy Engine (exist)

### 6. Resource Budgets with Hard Ceilings

**Brief reference**: §16 refinement — codify hard ceilings for RAM, VRAM, context length, queue depth, disk watermarks

**Implementation**:
- Extend `ResourceMonitor` with configurable hard ceilings
- Add `ResourceBudget` struct with RAM, VRAM, context, queue, disk limits
- Implement refusal logic when budgets are exceeded
- Add budget event logging to event store
- Integrate budget checks into Kernel task submission

**Dependencies**: ResourceMonitor, Event Store, Kernel (exist)

### 7. Project State Summaries

**Brief reference**: §6.5 — store derived summaries separately from raw events

**Implementation**:
- Add project summary generation from event log
- Implement summary caching with TTL
- Add `ProjectSummary` struct and storage
- Integrate with `ReplayEngine` for summary derivation
- Add summary update triggers on task completion

**Dependencies**: Event Store, Replay Engine (exist)

### 8. Execution Artifacts

**Brief reference**: §6.5 — store execution artifacts separately

**Implementation**:
- Add `Artifact` struct (tool output, file changes, etc.)
- Implement artifact storage (separate from event log)
- Add artifact indexing and retrieval
- Link artifacts to events via `EventPayload::ArtifactRecorded`
- Add artifact cleanup policy (age-based, size-based)

**Dependencies**: Event Store (exists)

### 9. Policy Decisions as Events

**Brief reference**: §7 control flow — audit records every transition

**Implementation**:
- Add `EventKind::PolicyDecision` variant
- Add `EventPayload::PolicyDecision` with decision details
- Log all policy evaluations as events
- Ensure policy decision events are append-only and durable
- Add policy decision replay capability

**Dependencies**: Event Store, Policy Engine (exist)

## Execution Order

1. **MCP Integration** (highest priority — explicitly called out in brief §6.11)
2. **ACP Integration** (high priority — explicitly called out in brief §6.11)
3. **Vision Provider** (high priority — required for multi-model architecture)
4. **Worker Isolation** (medium priority — security improvement)
5. **Manifest Lifecycle** (medium priority — governance improvement)
6. **Resource Budgets** (medium priority — stability improvement)
7. **Project State Summaries** (low priority — performance optimization)
8. **Execution Artifacts** (low priority — audit completeness)
9. **Policy Decisions as Events** (low priority — audit completeness)

## Validation

- All 684+ tests must continue to pass
- 0 clippy warnings
- Each new crate follows existing patterns (edition 2021, async-trait, proper error types)
- MCP/ACP requests must pass through Policy Engine and Capability Manager
- Vision provider must produce structured observations, not direct actions
- Worker isolation must not bypass capability checks
- Manifest state transitions must be validated and logged
- Resource budget refusals must be recorded as events

## Open Questions

1. Should MCP and ACP be separate crates or combined into one?
2. What is the exact Qwen3.5 9B model ID and API endpoint for the vision provider?
3. Should worker isolation use subprocess spawning or a worker pool pattern?
4. What signing mechanism should be used for manifests (none, simple hash, PKI)?
5. What are the default resource budget ceilings for the target hardware (16 GB RAM, 6 GB VRAM)?