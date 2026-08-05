# NexusAOS Issue Resolution Roadmap

> Resolves verified issues from the combined understanding document (docs/combined-understanding.md).
> Each item is tagged with priority, affected files, and a concrete acceptance criterion.

---

## P0 — Critical (system unusable or exploitable)

### 1. Fix README "Production Ready" badge → "Alpha"
- **Why**: The repo's own audit reports document critical wiring gaps. A "Production Ready" badge misrepresents stability.
- **File**: `README.md` (line 48)
- **Change**: Replace `Production Ready` with `Alpha` in the badge URL and the status table row.
- **Acceptance**: `grep "Production Ready" README.md` returns no results; `grep "Alpha" README.md` returns the badge and table entry.

### 2. Delete `scratch.rs` from workspace root
- **Why**: Scratch files leak internal exploration work, add noise to repository browsing, and suggest a missing `.gitignore` entry.
- **File**: `scratch.rs` (root)
- **Change**: Delete the file. Add `*.scratch.rs` to `.gitignore` if not already present.
- **Acceptance**: `ls scratch.rs` fails; `.gitignore` contains `*.scratch.rs`.

### 3. Add `[workspace.package]` to root `Cargo.toml`
- **Why**: Without shared metadata, each crate must duplicate version/license/edition fields, causing version drift.
- **File**: `Cargo.toml`
- **Change**: Add `[workspace.package]` section with `version = "0.1.0"`, `edition = "2021"`, `license = "MIT"`, `authors` from existing crate metadata. Remove duplicated fields from individual crate `Cargo.toml` files.
- **Acceptance**: `cargo metadata --format-version 1` shows workspace package metadata; `cargo clippy --workspace` reports 0 warnings.

### 4. Add `cargo-deny` and `cargo-audit` to CI
- **Why**: The clippy + test pipeline catches logic bugs but not supply-chain vulnerabilities.
- **File**: `.github/workflows/` (existing CI config)
- **Change**: Add `cargo-deny check` and `cargo audit` steps to the CI pipeline. Fail the build on license violations or CVE advisories.
- **Acceptance**: CI workflow file contains `cargo-deny` and `cargo-audit` steps; running them locally produces no violations.

### 5. Implement event store hash chaining
- **Why**: An attacker with write access to the SQLite database can silently modify historical events. Each event's checksum is independent, so tampering is undetectable.
- **File**: `crates/nexusaos-kernel/src/events.rs`, `crates/nexusaos-kernel/src/storage/event_store.rs`
- **Change**: Add `prev_hash: String` field to `Event`. In `compute_checksum()`, include `prev_hash` in the SHA-256 input. In `EventStore::append()`, set `prev_hash` to the checksum of the last event. On replay, verify each event's checksum against the previous event's checksum.
- **Acceptance**: `Event` struct has `prev_hash` field; `compute_checksum()` includes it; `event_store.rs` verifies chain on replay; tampering with any event causes checksum mismatch on subsequent events.

### 6. Fix ACP authentication — add SO_PEERCRED peer validation
- **Why**: Any process that can reach the Unix socket gets a fully-authenticated ACP session. No credential validation is performed.
- **File**: `crates/nexusaos-acp/src/server.rs`
- **Change**: On Unix socket connection, extract peer credentials via `SO_PEERCRED` (Linux). Validate the peer PID/UID against an allowlist or require a token challenge before granting session authentication. Add `authenticate` method that validates credentials before creating a session.
- **Acceptance**: `server.rs` calls `SO_PEERCRED` on new connections; unauthenticated peers are rejected; `validate_acp_request` requires valid peer credentials.

---

## P1 — High (security gaps or significant correctness issues)

### 7. Consolidate MCP + ACP into `nexusaos-protocols` crate
- **Why**: 14 crates is too granular for a solo project. MCP and ACP are both protocol adapters with similar structure (session management, capability checking, policy validation). Consolidating reduces compile-time friction and interface churn.
- **Files**: `crates/nexusaos-mcp/`, `crates/nexusaos-acp/`, `Cargo.toml`, `bin/nexusaos-cli/Cargo.toml`
- **Change**: Create `crates/nexusaos-protocols/` with `Cargo.toml` and `src/lib.rs`. Move MCP and ACP code into submodules (`mcp/`, `acp/`). Update workspace members, CLI binary deps, and all imports. Remove old crate directories.
- **Acceptance**: `cargo build --workspace` succeeds; `cargo test --workspace` passes; 16 workspace members become 14 (12 crates + 2 binaries).

### 8. Add task deduplication
- **Why**: Identical tasks create duplicate work, wasting resources and cluttering the event store.
- **File**: `crates/nexusaos-kernel/src/runtime/kernel.rs`
- **Change**: Add `HashMap<TaskHash, (TaskId, DateTime<Utc>)>` to `Kernel`. Hash task input text (SHA-256 of prompt + model role). In `submit_task()`, check if identical input arrived within `dedup_window_secs`; if so, return existing `TaskId` instead of creating a new task.
- **Acceptance**: `submit_task()` with identical prompt within dedup window returns existing TaskId; no duplicate `TaskCreated` events for identical inputs.

### 9. Add ToolBroker global timeout
- **Why**: Individual tools (filesystem, git, docker, search_fetch) have no timeout enforcement. A hung tool blocks the entire execution pipeline.
- **File**: `crates/nexusaos-kernel/src/tools/broker.rs`
- **Change**: Wrap `executor.execute()` in `tokio::time::timeout()` with a configurable global timeout (default 30s). On timeout, return `ToolResult { success: false, output: "Tool timed out" }`.
- **Acceptance**: `ToolBroker::execute()` wraps each call in `tokio::time::timeout()`; hung tools return failure instead of hanging indefinitely.

### 10. Add model-loading UX (status panel)
- **Why**: The README model table presents Gemma 4 12B + Qwen3-Coder 30B as a simultaneous stack, but they together exceed 16GB RAM. Users need to know which model is active and whether it fits.
- **File**: `crates/nexusaos-tui/src/` (TUI status panel)
- **Change**: Add a status panel showing: active model name, estimated load time, RAM pressure indicator. Gate 30B coder behind a VRAM check before loading.
- **Acceptance**: TUI shows active model and RAM pressure; attempting to load a model that exceeds available VRAM is blocked with a clear error.

---

## P2 — Medium (correctness & completeness)

### 11. Implement `confirm_task()` for AwaitingConfirmation → Executing
- **Why**: Currently `AwaitingConfirmation` is a dead-end state if the user doesn't explicitly confirm. Add `Kernel::confirm_task()` that transitions to `Executing`.
- **File**: `crates/nexusaos-kernel/src/runtime/kernel.rs`
- **Change**: `confirm_task()` already exists (line 1256). Verify it works end-to-end: transition from `AwaitingConfirmation` to `Executing`, then call `execute_task()`. Add a timeout: if `AwaitingConfirmation` persists beyond a configurable TTL, auto-transition to `Failed`.
- **Acceptance**: `confirm_task()` transitions state and triggers execution; unconfirmed tasks auto-fail after TTL.

### 12. Fix SnapshotStore `last_sequence = 0`
- **Why**: Snapshots with `last_sequence = 0` can't determine where to resume replay from the snapshot.
- **File**: `crates/nexusaos-kernel/src/storage/` (snapshot code)
- **Change**: Before saving a snapshot, populate `last_sequence` from the event store's maximum sequence number. On replay-from-snapshot, resume from `last_sequence + 1`.
- **Acceptance**: Snapshot `last_sequence` matches the event store's max sequence; replay-from-snapshot resumes at the correct position.

### 13. Replace Zig VT100 parser with `vte` crate
- **Why**: The `zig/` directory adds a Zig toolchain build dependency, an unsafe FFI boundary, and CI complexity. The `vte` crate (pure Rust, used in Alacritty and Zellij) handles all VT100/ANSI parsing with zero-allocation callbacks.
- **Files**: `zig/`, `crates/nexusaos-terminal/`
- **Change**: Remove `zig/` directory and its `build.zig`. Replace Zig FFI calls in `nexusaos-terminal` with direct `vte` crate usage. Update CI to remove Zig toolchain setup.
- **Acceptance**: `zig/` directory removed; `nexusaos-terminal` uses `vte` crate directly; CI no longer requires Zig; `cargo build --workspace` succeeds without Zig installed.

### 14. Add `[workspace.package]` shared metadata
- **Why**: Already covered in P0 #3. This is the same issue tracked from a different angle.
- **Status**: Resolved by P0 #3.

---

## P3 — Low (hygiene)

### 15. Fix README footer links (point to `nexusaos` org that doesn't exist)
- **Why**: Links in the README footer point to `https://github.com/nexusaos/NexusAOS` which is a different URL than `gaganjainse/nexus-kernel`.
- **File**: `README.md` (footer links)
- **Change**: Replace all `nexusaos/NexusAOS` URLs with `gaganjainse/nexus-kernel`.
- **Acceptance**: `grep "nexusaos/NexusAOS" README.md` returns no results.

### 16. Document `litellm_config.yaml` dependency or remove it
- **Why**: `litellm_config.yaml` at workspace root implies a Python proxy dependency that isn't mentioned in README prerequisites.
- **File**: `litellm_config.yaml` (root)
- **Change**: Either document it as a required service in README prerequisites, or remove the file and absorb its routing into the OpenAI-compat provider.
- **Acceptance**: README prerequisites mention litellm if kept; or file is removed and routing is absorbed.

### 17. Add `cargo-fmt` pre-commit hook or CI check
- **Why**: While `cargo +nightly fmt --check` passes, adding a pre-commit hook prevents formatting drift.
- **File**: `.git/hooks/pre-commit` or CI config
- **Change**: Add `cargo +nightly fmt --check` as a pre-commit hook or CI step.
- **Acceptance**: Pre-commit hook or CI step runs `cargo +nightly fmt --check` and fails on diffs.

---

## Dependency Order

```
P0 #1 (README badge)          ── independent
P0 #2 (delete scratch.rs)     ── independent
P0 #3 ([workspace.package])    ── independent
P0 #4 (cargo-deny/audit CI)    ── independent
P0 #5 (hash chaining)          ── independent
P0 #6 (ACP SO_PEERCRED)        ── independent
P1 #7 (consolidate MCP+ACP)    ── depends on P0 #5, #6 (must be done after those are stable)
P1 #8 (task dedup)             ── independent
P1 #9 (ToolBroker timeout)     ── independent
P1 #10 (model-loading UX)      ── independent
P2 #11 (confirm_task TTL)      ── independent
P2 #12 (SnapshotStore fix)     ── independent
P2 #13 (replace Zig with vte)  ── independent
P3 #15 (README footer links)   ── independent
P3 #16 (litellm_config)        ── independent
P3 #17 (fmt pre-commit)        ── independent
```

All P0 items are independent and can be parallelized. P1 items are also independent of each other. P2 and P3 items are independent.

---

## Validation

After all items are complete:
1. `cargo clippy --workspace --all-targets` → 0 warnings/errors
2. `cargo +nightly fmt --check` → 0 diffs
3. `cargo test --workspace` → all pass
4. `cargo build --workspace` → clean build
5. `grep "Production Ready" README.md` → no results
6. `ls scratch.rs` → fails
7. `grep "\[workspace.package\]" Cargo.toml` → found
8. `grep "cargo-deny\|cargo-audit" .github/workflows/*` → found
9. `grep "prev_hash" crates/nexusaos-kernel/src/events.rs` → found
10. `grep "SO_PEERCRED" crates/nexusaos-acp/src/server.rs` → found