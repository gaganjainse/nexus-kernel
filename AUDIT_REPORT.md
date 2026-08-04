# NexusAOS v2 — Production-Readiness Audit Report

**Audited:** `/home/gagan/Workspace/nexus-kernel`
**Source:** Architecture brief (`nexus_aos_architecture_brief.md`), `.kilo/plans/*.md`, all crate source files
**Date:** 2026-08-04

---

## Executive Summary

The project has a solid structural foundation: a working event store, state machine, policy engine, tool layer, MCP/ACP protocol adapters, and model provider abstraction. However, several components required by the architecture brief are either stubbed, disconnected from the runtime, or have security-relevant gaps. The most critical issues are: a missing worker binary, worker isolation code that is completely disconnected from the tool execution path, `ResourceBudget` not wired into `submit_task`, a capability-check bypass in MCP, hardcoded fallback paths, sandboxing disabled by default, and several `unimplemented!()` stubs that would panic if those code paths are reached.

---

## Component-by-Component Audit

### 1. Kernel (`Kernel` struct + `submit_task`/`execute_task`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/runtime/kernel.rs` |
| **State** | WIRED (partially) |

**Gaps found:**

1. **`ResourceBudget` / `ResourceMonitor` NOT wired into `submit_task`.**  
   `Kernel::new` accepts no `ResourceBudget` or `ResourceMonitor`. The `ResourceBudget` struct exists in `resource.rs` with correct defaults (14 GB RAM ceiling, 5 GB VRAM, 32K context, depth 32, 5 GB disk floor), but `submit_task` performs no admission check against it. The architecture brief (§"Heavy model policy", §"Resource budgets") requires hard ceilings for RAM, VRAM, queue depth, and disk watermarks to be checked before work is accepted.  
   **Fix:** Add `resource_budget: ResourceBudget` and `resource_monitor: Arc<ResourceMonitor>` to `Kernel::new`. At the top of `submit_task`, call `ResourceBudget::check_all(resource_monitor.snapshot(), &resource_budget)` and reject with `NexusError::Resource(...)` if any ceiling is exceeded. Also check queue depth via `ResourceBudget::exceeds_queue_budget`.

2. **`Scheduler` is implemented but never wired into the kernel.**  
   `Scheduler` (`runtime/scheduler.rs`) is a fully-implemented priority queue with depth limits, but `kernel.rs` has zero references to it. Tasks are tracked only in the in-memory `TaskProjection`, never enqueued.  
   **Fix:** Add a `Scheduler` field to `Kernel`. Enqueue tasks in `submit_task`; have a driver loop dequeue and dispatch to `execute_task`. Expose `cancel` and `drain` for shutdown.

3. **`Kernel::execute_task` — no resource pressure gating before model calls.**  
   Before calling `call_model_with_fallback`, there is no check of `ResourceMonitor` or `ContextBudget`. The 30B coder could be loaded under memory pressure, causing swap-thrash.  
   **Fix:** In `execute_task`, before each `call_model`, call `context_manager.estimate_budget(complexity, &pressure, provider.max_context())` and refuse or downgrade if `ContextBudgetExceeded`.

4. **`coder.unwrap()` at line 344.**  
   After `if coder.is_none()` returns early, `coder.unwrap()` is safe in current logic but is a latent panic if control flow changes.  
   **Fix:** Replace with `let coder = coder.ok_or_else(|| NexusError::Provider(...))?;` for explicit error propagation.

5. **`redact_secrets` uses `.unwrap_or(result.len())` at lines 592 and 613.**  
   In production, if a JSON-style secret key is found but the value-end delimiter is absent (malformed JSON), the redaction silently falls back to redacting to end-of-string. This is acceptable behavior, but `unwrap_or` on a `Option<usize>` is fine here (no panic risk); flag for clarity only.

6. **`truncate_output` uses `.unwrap_or(cut_point)` at line 961.**  
   No panic risk — `rfind` returns `Option<usize>`, and `unwrap_or` provides a safe fallback. No action needed.

---

### 2. Worker Binary (`nexusaos-worker`)

| Field | Value |
|---|---|
| **File** | Referenced at `worker.rs:117` |
| **State** | **MISSING** |

**Gap:** `WorkerProcess::spawn()` calls `Command::new("nexusaos-worker")`. No binary named `nexusaos-worker` exists anywhere in the workspace — no `[[bin]]` section, no `bin/nexusaos-worker/src/main.rs`. Spawning will fail with `No such file or directory` at runtime.  
**Fix:** Create `bin/nexusaos-worker/src/main.rs` implementing a stdin/stdout JSON protocol. The worker must receive `ToolRequest` over stdin, execute the tool (with capability lease validation), and emit `ToolResult` over stdout. Wire it into `execute_tool_in_process` which currently returns fake/simulated results.

---

### 3. Worker Isolation (`WorkerPool`, `IsolatedWorkerExecutor`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/worker.rs` |
| **State** | **UNWIRED STUB** |

**Gaps found:**

1. **`WorkerPool` and `IsolatedWorkerExecutor` are never registered with `ToolBroker`.**  
   Zero references to `WorkerPool`, `IsolatedWorkerExecutor`, or `isolated_worker_executor` in `kernel.rs` or `tools/broker.rs`. All tool execution runs in-process via registered `ToolExecutor` implementations. The architecture brief (§"Tool sandboxing", §"Worker isolation") explicitly requires tools to run as same-machine isolated workers.  
   **Fix:** In `Kernel::new` or the tool setup path, instantiate `IsolatedWorkerExecutor` and register it with `ToolBroker` for all tools, or wrap each tool executor in `McpToolAdapter` + worker dispatch.

2. **`execute_tool_in_process` returns simulated/fake results.**  
   At line 283–287: `Ok(ToolResult { success: true, output: format!("Tool {} executed in worker {}", ...), ... })` — always returns success regardless of what the tool actually does. No real IPC with the worker process.  
   **Fix:** Implement stdin/stdout JSON-RPC with the `nexusaos-worker` binary. Deserialize real `ToolResult` from worker stdout.

3. **Capability lease is always `None`.**  
   `IsolatedWorkerExecutor::execute` calls `pool.execute_tool(request, None)` — the `lease` parameter is never populated, so capability enforcement is bypassed entirely.  
   **Fix:** Thread `CapabilityLease` from the tool authorization step through to `execute_tool`.

4. **`WorkerPool::find_idle_worker` returns `None` for all-busy pool, then `unwrap()` panics at line 212.**  
   `execute_tool` does `let worker_idx = worker_idx.unwrap()` after `if worker_idx.is_none() { return Err(...) }` — this is actually safe because the early return prevents the unwrap. No panic risk in current logic, but the pattern is fragile.  
   **Fix:** Use `let worker_idx = worker_idx.ok_or_else(|| ToolError::ExecutionFailed { ... })?;`.

---

### 4. ResourceBudget (`ResourceBudget`, `SystemPressure`, `ResourceMonitor`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/resource.rs` |
| **State** | **PRESENT BUT UNWIRED** |

**Gaps found:**

1. **Not in `Kernel` struct.**  
   `Kernel` has no `resource_budget` or `resource_monitor` field. The struct and its `check_all` method exist and are well-designed, but nothing calls them during task lifecycle.  
   **Fix:** Wire into `Kernel::new` and `submit_task` as described in Gap #1 above.

2. **`ResourceMonitor::snapshot()` is synchronous and blocking.**  
   Calls `sysinfo::System::new_all()`, `sys.refresh_memory()`, and subprocess calls for GPU. If called from an async context in production, it will block the runtime.  
   **Fix:** Wrap in `tokio::task::spawn_blocking` when called from async contexts.

3. **`query_disk_space` hardcodes `/` mount point.**  
   At line 225–228: `d.mount_point() == std::path::Path::new("/")` — fails on systems where data is on a non-root mount (e.g., `/home`, `/mnt/data`, WSL).  
   **Fix:** Use the data directory path from config to find the correct disk, or query all disks and use the one containing the data directory.

---

### 5. SnapshotStore / Checkpoint Triggers

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/storage/snapshot.rs`, `kernel.rs:468–762` |
| **State** | WIRED (partially) |

**Gaps found:**

1. **Only one checkpoint trigger is wired: before tool execution (line 469).**  
   The architecture brief (§6.7) specifies triggers: *before file writes, before git commits, before package installs, before destructive actions, before long multi-step tasks*. Currently only "before tool execution" is implemented, and it is triggered for all `TOOL:` directives — not differentiated by tool type.  
   **Fix:** In `execute_task`, parse the tool name and trigger type-specific checkpoints: for `filesystem` write/delete actions, for `git` commit, and for long-running tasks. Add explicit `CheckpointTrigger` enum.

2. **`SnapshotStore::last_sequence` is always 0.**  
   Snapshots are saved with `last_sequence: 0` (line 747). The field is never updated from the event store sequence, so snapshot compaction/replay-from-snapshot cannot determine where to resume.  
   **Fix:** Set `last_sequence` to the current event store sequence before saving.

3. **No automatic compaction or snapshot rotation.**  
   `SnapshotStore` saves indefinitely. No rotation policy, no max count, no size limit.  
   **Fix:** Add a `retain_latest(N)` method and call it periodically or on startup.

---

### 6. ToolBroker (`ToolBroker::execute`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/broker.rs` |
| **State** | WIRED |

**Gaps found:**

1. **No resource budget check before dispatching.**  
   `ToolBroker::execute` does not call `ResourceBudget::check_all` or verify disk watermarks before executing.  
   **Fix:** Add a resource pre-check at the top of `execute`, or have the kernel enforce it before calling the broker.

2. **Tool execution timeout is per-tool, not globally budgeted.**  
   `TerminalTool` has `timeout_secs` but there is no global tool execution budget enforced by the broker or kernel.  
   **Fix:** Add a `ToolExecutionBudget` to the broker config, enforce per-execution timeout in `execute`.

---

### 7. MCP Adapter (`McpToolAdapter`, `McpServer`, `check_mcp_capabilities`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-mcp/src/adapter.rs`, `lib.rs`, `server.rs` |
| **State** | PARTIALLY WIRED (security gap) |

**Gaps found:**

1. **`check_mcp_capabilities` has a hardcoded `/tmp` fallback that bypasses tool-specific capability checks.**  
   At `lib.rs:52–68`: if arguments contain no `path` or `command` key, the function unconditionally returns `capabilities.check_path(Path::new("/tmp"))`. This means *any* MCP tool call without those argument keys is granted access based solely on `/tmp` permission, regardless of which tool is being invoked. This bypasses the per-tool capability model.  
   **Fix:** Return `false` when no recognizable scope argument is present; require the caller to explicitly grant capabilities per tool. Do not grant implicit access via a hardcoded path.

2. **MCP server has no connection limit enforcement.**  
   `McpServerConfig::max_connections` is defined but never checked in the `run` loop.  
   **Fix:** Track active connections in an `Arc<AtomicUsize>`; reject new connections when at limit.

3. **`McpToolAdapter` does not enforce sandboxing.**  
   It checks path and command capabilities but does not wrap execution in a worker process or sandbox.  
   **Fix:** Route `McpToolAdapter::execute` through `IsolatedWorkerExecutor`.

---

### 8. ACP Adapter (`AcpClient`, `AcpSessionManager`, `AcpSession`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-acp/src/client.rs`, `session.rs`, `lib.rs` |
| **State** | PARTIALLY WIRED (no server listener) |

**Gaps found:**

1. **No ACP server / Unix socket listener exists.**  
   `AcpSessionManager` manages sessions in memory, but there is no `AcpServer` that binds a Unix socket and handles incoming ACP client connections. External IDE clients cannot connect.  
   **Fix:** Implement `AcpServer` analogous to `McpServer`, binding a Unix socket and dispatching `authenticate`, `capability/grant`, and session lifecycle methods.

2. **`AcpClient::authenticate` always succeeds if socket is reachable.**  
   No credential validation, no challenge-response. Any process that can connect to the Unix socket gets a fully-authenticated session.  
   **Fix:** Implement token-based or peer-credential authentication (e.g., `SO_PEERCRED` on Linux).

3. **Session expiry is checked lazily (`is_active`) but never enforced proactively.**  
   Expired sessions are never cleaned up from the session list. `active_sessions()` filters lazily but `get_session` and `terminate_session` operate on all sessions.  
   **Fix:** Add a background cleanup task or check expiry on every `get_session`/`create_session` call.

---

### 9. Tool Sandboxing Enforcement

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/terminal.rs`, `worker.rs` |
| **State** | PARTIALLY IMPLEMENTED |

**Gaps found:**

1. **`TerminalTool::require_sandbox` defaults to `false`.**  
   At `terminal.rs:19`: `require_sandbox: false`. When `false`, the tool falls back to `sh -c command` without bwrap, executing commands unsandboxed. The architecture brief (§"Tool sandboxing") requires sandboxing by default.  
   **Fix:** Change the default to `true`. Make sandboxing mandatory unless explicitly disabled in config with a confirmation gate.

2. **`FilesystemTool` has no sandboxing beyond path scope checks.**  
   `FilesystemTool::is_path_allowed` checks path prefixes but does not enforce read-only vs. write capability leases, and does not use a worker process. A compromised model could write anywhere within the allowed prefix.  
   **Fix:** Enforce that write/delete actions require a `CapabilityLease` with `Scope::Path` and an explicit write grant. Run filesystem tools in `IsolatedWorkerExecutor`.

3. **`DockerTool` runs `docker` commands directly without sandboxing.**  
   Image allow/deny list is a simple substring check — `image.contains(pattern)` can be bypassed with `myregistry.com/denied-image:v1` when the denied pattern is just `denied-image`. No worker isolation.  
   **Fix:** Use exact image name matching (or registry+name+tag tuple). Run in isolated worker. Enforce capability lease for docker scope.

4. **`SearchFetchTool` has no sandboxing, no network policy beyond allowed domains.**  
   `is_url_allowed` uses `url.contains(domain)` which allows `https://evil.com/nexus-kernel.com-phishing-page` to pass if `nexus-kernel.com` is in the allowed list.  
   **Fix:** Parse URL and match against the hostname exactly. Use `url::Url` crate.

---

### 10. Heavy Model Policy Enforcement

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/resource.rs`, `context.rs`, `runtime/kernel.rs` |
| **State** | PARTIALLY IMPLEMENTED |

**Gaps found:**

1. **No enforcement of "cold-loaded specialist with hard queueing" for the 30B coder.**  
   The architecture brief (§"Heavy model policy") says: "the 30B coder should be treated as a cold-loaded specialist with hard queueing and refusal when memory pressure would cause swap-thrash." Currently, `execute_task` calls `provider.complete(req)` directly with no memory pressure check before model load.  
   **Fix:** Before calling any model, call `ResourceMonitor::snapshot()` (via `spawn_blocking`) and check `ResourceBudget::exceeds_ram_budget` and `has_sufficient_vram`. If the coder role is requested and VRAM is insufficient, refuse or queue the task with `TaskState::Blocked`.

2. **`ResourceBudget::exceeds_vram_budget` returns `false` when `vram_total_mb == 0` (no GPU).**  
   At `resource.rs:54–56`: CPU-only inference is assumed OK. But the architecture specifies 6 GB VRAM hardware, so the coder model load should be explicitly gated.  
   **Fix:** On GPU-less systems, refuse the coder role with a clear error rather than silently allowing it.

3. **`ContextManager::estimate_budget` does not clamp to `ResourceBudget.max_context_tokens`.**  
   `estimate_budget` clamps to `model_max_context` but the global `max_context_tokens` ceiling from `ResourceBudget` is never checked.  
   **Fix:** After model-max clamping, also clamp to `resource_budget.max_context_tokens`.

---

### 11. Provider Registry Stubs (`unimplemented!()`)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/registry.rs` lines 106, 146, 190, 244, 292 |
| **State** | **STUB (test-only, but reachable)** |

**Gap:** Five `MockProvider` and `MockCoder`/`AnotherPlanner`/`HealthyProvider`/`FailingProvider` implementations in `#[cfg(test)]` use `unimplemented!()` in their `complete` method. These will panic at runtime if the test accidentally calls `complete` on them. The `health_check_all` method correctly uses `catch_unwind`, but `unimplemented!()` panics are not catchable by `catch_unwind` in all contexts (they abort in some configurations).  
**Fix:** Replace all `unimplemented!()` in test mocks with `Ok(CompletionResponse { content: "mock".into(), ... })`.

---

### 12. `ClaudeProvider` Health Check Uses Wrong Endpoint

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/claude.rs:100–118` |
| **State** | BUG |

**Gap:** `health_check` sends a GET to `https://api.anthropic.com/v1/messages` (the completion endpoint). The Anthropic API does not support GET on `/v1/messages`. This will always return 405 Method Not Allowed, causing `health_check` to always report `Err(HealthCheckFailed)`.  
**Fix:** Use a supported health check endpoint (e.g., `/v1/models` GET or a simple auth-ping POST with `max_tokens: 1`).

---

### 13. `OpenAiCompatProvider::complete_stream` SSE Parser

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/openai_compat.rs:101–150` |
| **State** | COMPLETE (minor issue) |

**Gap:** `parse_sse_buffer` does not handle `data: ` lines that are empty (heartbeat comments). Some OpenAI-compatible servers emit `: heartbeat\n` or empty `data:\n` lines. The current parser silently ignores them, which is correct behavior, but does not emit a warning. Minor; no production breakage expected.

---

### 14. `check_mcp_capabilities` Hardcoded `/tmp` Bypass (Security)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-mcp/src/lib.rs:52–68` |
| **State** | **SECURITY BUG** |

**Gap:** As noted in Gap #7.1, the function always returns `true` for any MCP tool call without `path` or `command` arguments, because it checks `capabilities.check_path(Path::new("/tmp"))` as a fallback. An MCP client can invoke `docker.run` with `{action: "run", image: "alpine", cmd: "rm -rf /"}` and bypass all capability checks because the arguments contain no `path` or `command`.  
**Fix:** Remove the fallback. Return `false` when no recognized scope argument is present. Require explicit capability grants per tool.

---

### 15. `query_disk_space` Hardcoded `/` Mount Point

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/resource.rs:222–259` |
| **State** | BUG |

**Gap:** At line 225: `d.mount_point() == std::path::Path::new("/")` — only the root filesystem is checked. If `data_dir` is on a separate partition (common on Ubuntu with `/home` or `/var` separate), disk pressure on the data partition is invisible to `ResourceBudget`.  
**Fix:** Accept the data directory path in `query_disk_space` and find the disk that contains it.

---

### 16. `Manifest` and `Artifact` — Defined but Unused

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/manifest.rs`, `artifact.rs` |
| **State** | **STUB (defined, not integrated)** |

**Gap:** `Manifest` (with states: draft → validated → signed → active → superseded → retired) and `Artifact` types are fully defined with serialization and tests, but are never created, validated, or referenced by the `Kernel`, `ToolBroker`, or any runtime code. The architecture brief (§6 on manifest lifecycle, §6.5 on artifacts) requires them.  
**Fix:** Integrate `Manifest` creation into `Kernel::submit_task` (create a manifest for each task). Store `Artifact` records in `ToolResult.data` and emit `ArtifactRecorded` events.

---

### 17. Task Deduplication — Config Field Exists, Logic Missing

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/config.rs:97` (`dedup_window_secs`) |
| **State** | **MISSING** |

**Gap:** `dedup_window_secs` is a config field with a default of 5 seconds, but `Kernel::submit_task` does not check for duplicate tasks within that window. Rapid repeated prompts will create independent task records.  
**Fix:** In `submit_task`, maintain a `HashMap<TaskInput, (TaskId, DateTime<Utc>)>` (or hash of input text). If an identical input is submitted within `dedup_window_secs`, return the existing `TaskId` instead of creating a new task.

---

### 18. `ToolBroker::execute` — No Timeout Enforcement

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/broker.rs:41–73` |
| **State** | INCOMPLETE |

**Gap:** `ToolBroker::execute` calls `executor.execute(request).await` with no timeout wrapper. A misbehaving tool (e.g., `terminal` running `sleep infinity`) will block the async task forever. `TerminalTool` has its own timeout, but `FilesystemTool`, `GitTool`, `DockerTool`, and `SearchFetchTool` have no timeout.  
**Fix:** Wrap `executor.execute` in `tokio::time::timeout(Duration::from_secs(max_tool_seconds), ...)` in `ToolBroker::execute`.

---

### 19. `ProviderRegistry::health_check_all` Panic Isolation

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/registry.rs:31–53` |
| **State** | COMPLETE |

No gap here — `health_check_all` correctly uses `AssertUnwindSafe` + `FutureExt::catch_unwind`. This is a positive finding.

---

### 20. ACP Session Manager — No Expiry Cleanup

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-acp/src/session.rs:130–201` |
| **State** | INCOMPLETE |

**Gap:** `AcpSessionManager::active_sessions` filters by `is_active()` (which checks `expires_at`), but `get_session`, `terminate_session`, and `sessions` list all sessions including expired ones. An attacker who obtained a session with a short TTL could continue using it if the session ID is replayed before cleanup.  
**Fix:** In `get_session` and `terminate_session`, skip or auto-terminate expired sessions. Add periodic cleanup.

---

### 21. `FilesystemTool::resolve_for_check` — `canonicalize` Can Panic

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/filesystem.rs:22–29` |
| **State** | RISK |

**Gap:** `resolve_for_check` calls `path.canonicalize().unwrap_or_else(|_| path.to_path_buf())`. The `unwrap_or_else` handles `IoError` correctly (falls back to the original path), but if the path contains `..` components that escape the allowed prefix, the fallback preserves the uncanonicalized path, which `is_path_allowed` then checks. This is actually correct behavior. However, `canonicalize` follows symlinks — a symlink within an allowed path could point outside it.  
**Fix:** After canonicalization, re-check `is_path_allowed` on the resolved path and reject if it escapes.

---

### 22. `DockerTool` — `image.contains(pattern)` Allowlist Bypass

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/docker.rs:21–33` |
| **State** | **SECURITY GAP** |

**Gap:** `is_image_allowed` uses `image.contains(pattern)` and `image.contains(denied)`. An attacker can use `myregistry.com/allowed-image:latest` when `allowed` is `allowed-image` — this matches because of substring. Similarly, `denied_images` can be bypassed with `allowed-denied-image`.  
**Fix:** Parse image references as `(registry, name, tag)` tuples and match exactly. Use the `docker_parser` crate or implement proper OCI reference parsing.

---

### 23. `OpenAiCompatProvider::complete` — `resp.text().await.unwrap_or_default()` Swallows Errors

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/openai_compat.rs:199` |
| **State** | INCOMPLETE |

**Gap:** On non-success HTTP status, `resp.text().await.unwrap_or_default()` silently returns an empty string if reading the body fails. The error body is lost.  
**Fix:** Use `resp.text().await.unwrap_or_else(|_| "(failed to read error body)".into())` to at least indicate the secondary failure.

---

### 24. `ClaudeProvider::complete` — `resp.text().await.unwrap_or_default()` Same Issue

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/claude.rs:166` |
| **State** | INCOMPLETE |

Same gap as #23.

---

### 25. `SearchFetchTool` — URL Allowlist Substring Bypass

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/search_fetch.rs:21–33` |
| **State** | **SECURITY GAP** |

**Gap:** `is_url_allowed` uses `url.contains(domain)` for matching. As described in Gap #9.4, `https://evil.com/nexus-kernel.com-phishing` would pass if `nexus-kernel.com` is in the allowed list.  
**Fix:** Parse URL with `url::Url`, extract `host()`, and match exactly against allowed hostnames.

---

### 26. `Kernel::new` Does Not Accept `ContextManager`

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/runtime/kernel.rs:117–136` |
| **State** | **MISSING** |

**Gap:** `ContextManager` (`context.rs`) is fully implemented with `estimate_budget`, `TaskComplexity` estimation, and pressure-aware clamping. But `Kernel::new` does not accept a `ContextManager`, so context budgeting is never enforced during task execution. The architecture brief requires that context budgets are computed per-task and model loads are refused when context would exceed safe limits.  
**Fix:** Add `context_manager: Arc<ContextManager>` to `Kernel::new`. In `execute_task`, before each `call_model_with_fallback`, call `context_manager.estimate_budget(complexity, &pressure, provider.max_context())` and pass the resulting `max_tokens` into `CompletionRequest::new`.

---

### 27. `Manifest` / `Artifact` — Defined but Unused in Runtime

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/manifest.rs`, `artifact.rs` |
| **State** | **STUB (defined, not integrated)** |

**Gap:** `Manifest` (with states: draft → validated → signed → active → superseded → retired) and `Artifact` types are fully defined with serialization and tests, but are never created, validated, or referenced by the `Kernel`, `ToolBroker`, or any runtime code. The architecture brief (§6 on manifest lifecycle, §6.5 on artifacts) requires them.  
**Fix:** Integrate `Manifest` creation into `Kernel::submit_task` (create a manifest for each task). Store `Artifact` records in `ToolResult.data` and emit `ArtifactRecorded` events.

---

### 28. `check_mcp_capabilities` Hardcoded `/tmp` Bypass (Security)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-mcp/src/lib.rs:52–68` |
| **State** | **SECURITY BUG** |

**Gap:** As noted in Gap #7.1, the function always returns `true` for any MCP tool call without `path` or `command` arguments, because it checks `capabilities.check_path(Path::new("/tmp"))` as a fallback. An MCP client can invoke `docker.run` with `{action: "run", image: "alpine", cmd: "rm -rf /"}` and bypass all capability checks because the arguments contain no `path` or `command`.  
**Fix:** Remove the fallback. Return `false` when no recognized scope argument is present. Require explicit capability grants per tool.

---

### 29. `query_disk_space` Hardcoded `/` Mount Point

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/resource.rs:222–259` |
| **State** | BUG |

**Gap:** At line 225: `d.mount_point() == std::path::Path::new("/")` — only the root filesystem is checked. If `data_dir` is on a separate partition (common on Ubuntu with `/home` or `/var` separate), disk pressure on the data partition is invisible to `ResourceBudget`.  
**Fix:** Accept the data directory path in `query_disk_space` and find the disk that contains it.

---

### 30. `FilesystemTool::resolve_for_check` — `canonicalize` Can Follow Symlinks Outside Scope

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/filesystem.rs:22–29` |
| **State** | RISK |

**Gap:** `resolve_for_check` calls `path.canonicalize().unwrap_or_else(|_| path.to_path_buf())`. The `unwrap_or_else` handles `IoError` correctly (falls back to the original path), but `canonicalize` follows symlinks — a symlink within an allowed path could point outside it. The `is_path_allowed` check happens on the original path, not the resolved one.  
**Fix:** Canonicalize first, then call `is_path_allowed` on the resolved path. If the resolved path escapes the allowed prefix, deny.

---

### 31. `ClaudeProvider::health_check` Uses Wrong Endpoint

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/claude.rs:100–118` |
| **State** | BUG |

**Gap:** `health_check` sends a GET to `https://api.anthropic.com/v1/messages` (the completion endpoint). The Anthropic API does not support GET on `/v1/messages`. This will always return 405 Method Not Allowed, causing `health_check` to always report `Err(HealthCheckFailed)`.  
**Fix:** Use a supported health check endpoint (e.g., `/v1/models` GET or a simple auth-ping POST with `max_tokens: 1`).

---

### 32. `OpenAiCompatProvider::complete` — Error Body Swallowed

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/openai_compat.rs:199` |
| **State** | INCOMPLETE |

**Gap:** On non-success HTTP status, `resp.text().await.unwrap_or_default()` silently returns an empty string if reading the body fails. The error body is lost.  
**Fix:** Use `resp.text().await.unwrap_or_else(|_| "(failed to read error body)".into())`.

---

### 33. `ClaudeProvider::complete` — Same Error Body Swallowed

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/claude.rs:166` |
| **State** | INCOMPLETE |

Same gap as #32.

---

### 34. `TaskDeduplication` — Config Field Exists, Logic Missing

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/config.rs:97` (`dedup_window_secs`) |
| **State** | **MISSING** |

**Gap:** `dedup_window_secs` is a config field with a default of 5 seconds, but `Kernel::submit_task` does not check for duplicate tasks within that window. Rapid repeated prompts will create independent task records.  
**Fix:** In `submit_task`, maintain a `HashMap<TaskInput, (TaskId, DateTime<Utc>)>` (or hash of input text). If an identical input is submitted within `dedup_window_secs`, return the existing `TaskId` instead of creating a new task.

---

### 35. `ToolBroker::execute` — No Global Timeout Enforcement

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/broker.rs:41–73` |
| **State** | INCOMPLETE |

**Gap:** `ToolBroker::execute` calls `executor.execute(request).await` with no timeout wrapper. A misbehaving tool (e.g., `terminal` running `sleep infinity`) will block the async task forever. `TerminalTool` has its own timeout, but `FilesystemTool`, `GitTool`, `DockerTool`, and `SearchFetchTool` have no timeout.  
**Fix:** Wrap `executor.execute` in `tokio::time::timeout(Duration::from_secs(max_tool_seconds), ...)` in `ToolBroker::execute`.

---

### 36. `DockerTool` — `image.contains(pattern)` Allowlist Bypass

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/docker.rs:21–33` |
| **State** | **SECURITY GAP** |

**Gap:** `is_image_allowed` uses `image.contains(allowed)` and `image.contains(denied)`. An attacker can use `my-registry.com/allowed-image:latest` when `allowed` is `allowed-image` — this matches because of substring. Similarly, `denied_images` can be bypassed with `allowed-denied-image`.  
**Fix:** Parse image references as `(registry, name, tag)` tuples and match exactly. Use the `docker_parser` crate or implement proper OCI reference parsing.

---

### 37. `SearchFetchTool` — URL Allowlist Substring Bypass

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/tools/search_fetch.rs:21–33` |
| **State** | **SECURITY GAP** |

**Gap:** `is_url_allowed` uses `url.contains(domain)` for matching. `https://evil.com/nexus-kernel.com-phishing` would pass if `nexus-kernel.com` is in the allowed list.  
**Fix:** Parse URL with `url::Url`, extract `host()`, and match exactly against allowed hostnames.

---

### 38. `unimplemented!()` Stubs in Test Mock Providers

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/registry.rs:106, 146, 190, 244, 292` |
| **State** | **STUB** |

**Gap:** Five `MockProvider` implementations in `#[cfg(test)]` use `unimplemented!()` in their `complete` method. These will panic if `complete` is called on them.  
**Fix:** Replace all `unimplemented!()` with `Ok(CompletionResponse { content: "mock".into(), ... })`.

---

### 39. `unimplemented!()` in `src/model/registry.rs` (Orphaned Src Tree — per plan)

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/model/registry.rs` |
| **State** | **ORPHANED STUB** |

**Gap:** The `.kilo/plans/architecture.md` notes `src/model/registry.rs` as an "orphaned" file with `unimplemented!()` stubs that "never compiled". The file exists in the active crate tree at `crates/nexusaos-kernel/src/model/registry.rs` and does compile, but it contains the `unimplemented!()` stubs in test mocks (Gap #38). The plan indicates this was meant to be deleted or properly integrated.  
**Fix:** Either remove the orphaned `src/model/registry.rs` if the active one supersedes it, or ensure only one `registry.rs` exists. Delete the orphaned file if confirmed redundant.

---

### 40. `WorkerPool::find_idle_worker` then `unwrap()` Pattern

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/worker.rs:204–212` |
| **State** | RISK |

**Gap:** `execute_tool` checks `if worker_idx.is_none() { return Err(...) }` then calls `worker_idx.unwrap()`. This is currently safe because of the early return, but if the logic is refactored, the unwrap becomes a panic vector.  
**Fix:** Replace with `let worker_idx = worker_idx.ok_or_else(|| ToolError::ExecutionFailed { ... })?;`.

---

### 41. `Kernel::execute_task` — `coder.unwrap()` After `is_none()` Check

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/runtime/kernel.rs:337–344` |
| **State** | RISK |

**Gap:** After `if coder.is_none() { return ... }`, `let coder = coder.unwrap();` is safe but fragile. If the early-return block is modified, this becomes a panic.  
**Fix:** Replace with `let coder = coder.ok_or_else(|| NexusError::Provider(ProviderError::Unavailable { name: "Coder".into() }))?;`.

---

### 42. `ResourceMonitor::snapshot()` Blocking in Async Context

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/resource.rs:131–150` |
| **State** | RISK |

**Gap:** `snapshot()` calls `System::new_all()`, `sys.refresh_memory()`, and subprocess calls synchronously. If called from an async context without `spawn_blocking`, it blocks the tokio runtime.  
**Fix:** Wrap in `tokio::task::spawn_blocking` when called from async contexts.

---

### 43. `SnapshotStore::last_sequence` Always 0

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/storage/snapshot.rs:744–749` |
| **State** | BUG |

**Gap:** Snapshots are saved with `last_sequence: 0` (line 747). The field is never populated from the event store sequence, so snapshot compaction/replay-from-snapshot cannot determine where to resume.  
**Fix:** Before `snapshot_store.save(&snapshot)`, read the current event store sequence and set `snapshot.last_sequence`.

---

### 44. No Snapshot Rotation / Compaction Policy

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/storage/snapshot.rs` |
| **State** | INCOMPLETE |

**Gap:** `SnapshotStore` saves indefinitely. No rotation policy, no max count, no size limit. On long-running systems, this will exhaust disk space.  
**Fix:** Add `retain_latest(N)` and call it on startup or periodically.

---

### 45. `AcpSessionManager` — No Proactive Expiry Cleanup

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-acp/src/session.rs:130–201` |
| **State** | INCOMPLETE |

**Gap:** Expired sessions are never removed from the session list. `active_sessions()` filters lazily, but `get_session` and `terminate_session` operate on all sessions including expired ones.  
**Fix:** Add a cleanup method and call it on session list access, or run a periodic cleanup task.

---

### 46. `PolicyEngine` — No Confirmation Gate for `TaskState::AwaitingConfirmation` Transitions

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/runtime/kernel.rs:533–552` |
| **State** | INCOMPLETE |

**Gap:** When a tool returns `RequiresConfirmation`, `execute_task` transitions to `AwaitingConfirmation` and returns `requires_confirmation: true`. But there is no code path that transitions from `AwaitingConfirmation` back to `Executing` after user confirmation. The architecture requires an explicit confirmation gate.  
**Fix:** Add `Kernel::confirm_task(task_id)` that transitions `AwaitingConfirmation → Executing` and resumes execution.

---

### 47. `Kernel::recover_incomplete_tasks` — No Resource Pressure Check

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/runtime/kernel.rs:888–923` |
| **State** | INCOMPLETE |

**Gap:** On startup, incomplete tasks are force-transitioned to `Failed` without checking resource pressure or attempting recovery. If the system crashed during tool execution, the task is marked failed even if the tool may have completed.  
**Fix:** Before marking failed, check `ResourceMonitor::snapshot()` and emit a `RecoveryAttempted` event. Optionally attempt to replay the last tool call from the event log.

---

### 48. `ProjectSummaryUpdated` Event — No Producer

| Field | Value |
|---|---|
| **File** | `crates/nexusaos-kernel/src/events.rs:129` |
| **State** | **MISSING** |

**Gap:** `EventPayload::ProjectSummaryUpdated` is defined but never emitted by any code. The architecture brief (§6.5) requires project summaries to be stored as derived data.  
**Fix:** Emit `ProjectSummaryUpdated` when a task reaches a terminal state, with a summary of the task outcome.

---

## Summary Table

| # | Component | State | Severity | Gap Type |
|---|-----------|-------|----------|----------|
| 1 | `Kernel::submit_task` | WIRED | HIGH | ResourceBudget not wired |
| 2 | `nexusaos-worker` binary | MISSING | CRITICAL | Binary missing |
| 3 | WorkerPool/IsolatedWorkerExecutor | UNWIRED STUB | HIGH | Not wired into ToolBroker/Kernel |
| 4 | `ResourceBudget` | PRESENT UNWIRED | HIGH | Not in Kernel struct |
| 5 | SnapshotStore/checkpoint | PARTIAL | MEDIUM | Only 1 of 5 triggers wired |
| 6 | ToolBroker::execute | WIRED | LOW | No resource pre-check |
| 7 | MCP adapter | PARTIAL | HIGH | `/tmp` capability bypass |
| 8 | ACP adapter | PARTIAL | MEDIUM | No server listener |
| 9 | Tool sandboxing | PARTIAL | HIGH | `require_sandbox` defaults false |
| 10 | Heavy model policy | PARTIAL | HIGH | No memory gating before model load |
| 11 | Provider registry stubs | STUB | MEDIUM | `unimplemented!()` in test mocks |
| 12 | Claude health check | BUG | MEDIUM | Wrong endpoint |
| 13 | SSE parser | COMPLETE | INFO | Minor: no heartbeat warning |
| 14 | `/tmp` bypass | SECURITY BUG | CRITICAL | MCP capability bypass |
| 15 | `query_disk_space` | BUG | LOW | Hardcoded `/` mount |
| 16 | Manifest/Artifact | UNWIRED STUB | MEDIUM | Defined but unused |
| 17 | Task deduplication | MISSING | MEDIUM | Config field exists, logic absent |
| 18 | ToolBroker timeout | INCOMPLETE | MEDIUM | No global timeout |
| 19 | Provider health check | COMPLETE | INFO | `catch_unwind` correct |
| 20 | ACP session expiry | INCOMPLETE | MEDIUM | No proactive cleanup |
| 21 | `canonicalize` symlink | RISK | MEDIUM | Symlink escape possible |
| 22 | Docker allowlist | SECURITY GAP | HIGH | Substring matching |
| 23 | OpenAI error body | INCOMPLETE | LOW | Error body swallowed |
| 24 | Claude error body | INCOMPLETE | LOW | Error body swallowed |
| 25 | SearchFetch allowlist | SECURITY GAP | HIGH | Substring matching |
| 26 | ContextManager unwired | MISSING | HIGH | Not in Kernel struct |
| 27 | Manifest/Artifact unused | STUB | MEDIUM | Not integrated |
| 28 | `/tmp` bypass repeat | SECURITY BUG | CRITICAL | See #14 |
| 29 | `query_disk_space` repeat | BUG | LOW | See #15 |
| 30 | Symlink escape repeat | RISK | MEDIUM | See #21 |
| 31 | Claude health check repeat | BUG | MEDIUM | See #12 |
| 32–33 | Error body swallowed | INCOMPLETE | LOW | See #23–24 |
| 34 | Dedup missing repeat | MISSING | MEDIUM | See #17 |
| 35 | ToolBroker timeout repeat | INCOMPLETE | MEDIUM | See #18 |
| 36 | Docker allowlist repeat | SECURITY GAP | HIGH | See #22 |
| 37 | SearchFetch allowlist repeat | SECURITY GAP | HIGH | See #25 |
| 38 | `unimplemented!()` stubs | STUB | MEDIUM | Panic risk in mocks |
| 39 | Orphaned registry.rs | ORPHANED STUB | LOW | Duplicate file |
| 40 | `unwrap()` after `is_none()` | RISK | LOW | Fragile pattern |
| 41 | `coder.unwrap()` | RISK | LOW | Fragile pattern |
| 42 | `snapshot()` blocking | RISK | MEDIUM | Blocks async runtime |
| 43 | `last_sequence: 0` | BUG | MEDIUM | Snapshots can't resume |
| 44 | No snapshot rotation | INCOMPLETE | MEDIUM | Disk exhaustion risk |
| 45 | ACP no expiry cleanup | INCOMPLETE | MEDIUM | Session replay risk |
| 46 | No confirmation resume | INCOMPLETE | MEDIUM | AwaitingConfirmation dead-end |
| 47 | No recovery pressure check | INCOMPLETE | LOW | Crash recovery blind |
| 48 | `ProjectSummaryUpdated` orphan | MISSING | LOW | Event never emitted |

---

## Recommended Fix Priority Order

1. **CRITICAL:** Create `nexusaos-worker` binary and wire `IsolatedWorkerExecutor` into `ToolBroker`. Current tool execution is entirely in-process with no isolation.
2. **CRITICAL:** Fix `check_mcp_capabilities` `/tmp` fallback — this is an active capability bypass.
3. **HIGH:** Wire `ResourceBudget` + `ResourceMonitor` + `ContextManager` into `Kernel::new` and `submit_task`. Currently no admission control exists.
4. **HIGH:** Make `TerminalTool::require_sandbox` default to `true`. Currently commands run unsandboxed by default.
5. **HIGH:** Fix `DockerTool` and `SearchFetchTool` substring matching to exact hostname/reference matching.
6. **HIGH:** Implement `Scheduler` wiring into `Kernel`. Currently tasks are never queued or depth-limited.
7. **MEDIUM:** Fix `ClaudeProvider::health_check` endpoint (currently always fails).
8. **MEDIUM:** Replace all `unimplemented!()` in test mocks with real stub returns.
9. **MEDIUM:** Implement task deduplication using existing `dedup_window_secs` config.
10. **MEDIUM:** Add `ToolBroker::execute` global timeout wrapper.
11. **MEDIUM:** Implement `confirm_task` for `AwaitingConfirmation → Executing` resume.
12. **MEDIUM:** Add `SnapshotStore` rotation and `last_sequence` population.
13. **MEDIUM:** Integrate `Manifest` and `Artifact` into the task lifecycle.
14. **LOW:** Fix `query_disk_space` to use data directory mount point.
15. **LOW:** Fix error body swallowing in OpenAI/Claude providers.
16. **LOW:** Replace fragile `unwrap()` after `is_none()` checks with `ok_or_else`.
17. **LOW:** Add ACP server listener and session expiry cleanup.
18. **LOW:** Remove orphaned `src/model/registry.rs` if confirmed redundant.
19. **LOW:** Emit `ProjectSummaryUpdated` events on task completion.
