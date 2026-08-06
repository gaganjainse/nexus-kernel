# workspacectl Agent Execution Plan

## 1. Current state
The project is not finished. The last safe point is a documented scaffold and the design/spec layer. There is no verified production build yet. The next phase is implementation in small, testable increments.

## 2. Primary execution strategy
Build the project as a real Rust workspace, not as a monolithic script. Work in the following sequence:
1. Create the repository layout.
2. Implement shared types.
3. Implement deterministic classification.
4. Implement scan and plan.
5. Implement execution and rollback.
6. Implement watch daemon.
7. Implement optional AI integration.
8. Add tests.
9. Add docs and packaging.
10. Build, verify, and release.

## 3. Milestones and gate checks
### Milestone 0: repository bootstrap
Deliverables:
- Cargo workspace
- root README
- license
- attribution notes
- sample config
- baseline CI workflow
- formatting/linting hooks

Gate to proceed:
- repository builds at least a placeholder binary
- formatting passes
- docs outline exists

### Milestone 1: core types and config
Deliverables:
- config parser
- shared error types
- journal entry types
- file category enums
- path safety utilities
- ignore rules

Gate to proceed:
- config tests pass
- serialization roundtrip tests pass
- hidden/system path exclusion tests pass

### Milestone 2: scanner and classifier
Deliverables:
- recursive scanner
- metadata reader
- extension and sentinel classification
- deterministic rule engine
- confidence scoring

Gate to proceed:
- scan output is stable and structured
- project detection tests pass
- ignore rules are enforced

### Milestone 3: planner
Deliverables:
- destination router
- plan diff representation
- conflict resolver
- dry-run preview
- no side effects

Gate to proceed:
- plan generation is deterministic
- dry-run and repeated-run equivalence tests pass
- conflicts are handled without overwrite

### Milestone 4: executor and journal
Deliverables:
- atomic move executor
- append-only journal
- backup record for rollback
- action status reporting

Gate to proceed:
- every move is journaled
- rollback of sample moves works
- crash-safe append verified

### Milestone 5: watch daemon
Deliverables:
- filesystem watch backend
- debounce layer
- queue processor
- daemon process
- systemd user service

Gate to proceed:
- watcher handles bursts without duplicate actions
- daemon can restart cleanly
- watch mode falls back to scan mode if needed

### Milestone 6: optional AI classifier
Deliverables:
- local backend adapter
- ambiguous-case prompt builder
- safe parser for model output
- deterministic fallback behavior

Gate to proceed:
- AI is optional
- system still works offline
- ambiguity threshold behavior is tested

### Milestone 7: packaging and release
Deliverables:
- installer script
- release zip
- docs
- CI artifact
- checksums

Gate to proceed:
- clean build from fresh clone
- documented install path
- final archive contains all required files

## 4. Detailed implementation steps
### Step A: create the workspace
- Initialize `Cargo.toml` at the root.
- Add crates for CLI, daemon, core logic, config, journal, watch, AI integration, test utilities, and packaging helpers.
- Add `cargo fmt` and `cargo clippy` checks to CI.

Fallback if workspace setup is blocked:
- Keep a single root crate temporarily, but only as a short-lived bridge.

### Step B: implement shared domain types
- File categories.
- Rule matches.
- Plan actions.
- Journal records.
- Conflict strategy.
- Config schema.

Fallback if schema complexity grows:
- Split config into small nested tables.

### Step C: implement ignore and safety filters
- Hardcode hidden config paths to skip.
- Skip browser partial downloads.
- Skip lock files and temp files.
- Skip system paths unless a special allowlist exists.

Fallback if a path is ambiguous:
- leave it unchanged and report it.

### Step D: implement deterministic classifier
- Start with extension rules.
- Add project marker rules.
- Add size and metadata heuristics.
- Add confidence output.

Fallback if heuristics conflict:
- choose the more conservative category or `unknown`.

### Step E: implement planner
- Map category to workspace destination.
- Resolve destination path.
- Apply conflict strategy.
- Emit plan summary.

Fallback if destination is unavailable:
- archive or skip; never overwrite.

### Step F: implement executor and rollback
- Move file atomically where possible.
- Write journal record before/after.
- Restore from journal on rollback.

Fallback if atomic move is impossible:
- only use a copy-verify-remove path if the config explicitly allows cross-filesystem fallback.

### Step G: implement watcher
- Use `notify` or `inotify`.
- Debounce events.
- Filter incomplete files.
- Queue stable items for classification.

Fallback if watch backend fails:
- switch to periodic scan mode.

### Step H: optional AI adapter
- Accept a local OpenAI-compatible endpoint.
- Use only for ambiguous files.
- Parse model response conservatively.

Fallback if model output is invalid:
- fall back to deterministic rules.

### Step I: docs and service
- Document every command.
- Document rollback behavior.
- Provide `systemd --user` unit.
- Provide install and uninstall instructions.

Fallback if docs are incomplete:
- block release until docs are finished.

### Step J: final release packaging
- Verify tests.
- Build release binaries.
- Generate zip.
- Include config examples and attribution.
- Tag the release.

## 5. Exception handling matrix
### Permission denied
- Report clearly.
- Skip the file.
- Do not continue with a dangerous fallback unless the user explicitly configured a safe alternative.

### File disappears mid-scan
- Mark as unstable.
- Retry once after debounce.
- If still missing, ignore.

### Journal write failure
- Abort the affected action.
- Preserve source file.
- Surface the failure.

### Destination conflict
- Use suffix/hash/archive policy.
- Never replace existing file.

### Partial downloads
- Treat as transient.
- Ignore by pattern.

### AI backend failure
- Continue with rule-based classification.

### Rollback conflict
- Restore to a conflict folder or suffixed path.
- Never clobber newer data.

## 6. Stop conditions
Stop and report before continuing if:
- tests fail,
- build breaks,
- rollback is unverified,
- watch mode is unstable,
- docs are inconsistent with implementation,
- packaging is incomplete,
- or the release archive cannot be reproduced.

## 7. Done criteria
The project is done only when:
- it compiles,
- tests pass,
- docs exist,
- service files exist,
- install script exists,
- rollback works,
- watch works,
- and a real zip archive is produced.

