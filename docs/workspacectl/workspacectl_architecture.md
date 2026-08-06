# workspacectl Architecture

## 1. High-level architecture
The system should be split into a small set of focused layers:

1. **CLI layer** (`workspacectl`)
   - Parses arguments.
   - Loads config.
   - Dispatches commands.
   - Presents plans, actions, and reports.

2. **Daemon layer** (`workspaced`)
   - Watches filesystem events.
   - Debounces events.
   - Submits scan/plan/organize tasks.
   - Maintains runtime state.

3. **Core engine**
   - Scanner.
   - Classifier.
   - Planner.
   - Executor.
   - Rollback journal.
   - Conflict resolver.
   - Rule engine.

4. **Storage layer**
   - Configuration file.
   - JSONL or SQLite journal.
   - Cache for hashes and prior decisions.
   - Optional rule/learning store.

5. **Integration layer**
   - Optional local AI backend.
   - systemd user service.
   - CI.
   - Installer scripts.

## 2. Suggested Rust workspace layout
```text
workspacectl/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── workspacectl-cli/
│   ├── workspaced-daemon/
│   ├── workspace-core/
│   ├── workspace-config/
│   ├── workspace-rules/
│   ├── workspace-journal/
│   ├── workspace-watch/
│   ├── workspace-ai/
│   ├── workspace-testkit/
│   └── workspace-packaging/
├── docs/
├── scripts/
├── examples/
├── tests/
└── .github/workflows/
```

## 3. Core module responsibilities
### Scanner
- Walks directories.
- Gathers metadata.
- Detects candidates for classification.
- Skips ignored and hidden paths.
- Produces a normalized inventory.

### Classifier
- Runs deterministic rules first.
- Assigns a category and confidence.
- Produces explanation strings for each decision.
- Optionally calls a local AI backend for ambiguous cases only.

### Planner
- Converts classified items into a move plan.
- Resolves destination paths.
- Handles conflicts.
- Keeps the plan pure and side-effect free.

### Executor
- Applies approved plans.
- Uses atomic rename/move where possible.
- Records journal entries for every action.
- Aborts safely if an invariant is violated.

### Rollback journal
- Stores every action and enough metadata to undo it.
- Should support append-only writes.
- Should survive crashes and partial failures.

### Watcher
- Emits stable file events.
- Debounces rapid writes.
- Waits for file stability before action.
- Uses a queue to avoid concurrent conflicting moves.

### Rule engine
- Maintains ordered routing rules.
- Supports extension, path, regex, size, and metadata conditions.
- Produces deterministic decisions.

### Conflict resolver
- Prevents overwrites.
- Chooses suffix/hash/archive behavior.
- Makes collisions visible in output and journal.

## 4. Data flow
1. User runs `scan` or `plan`.
2. CLI loads config.
3. Scanner inspects the source directories.
4. Classifier assigns a category.
5. Planner resolves a destination.
6. Conflict resolver checks target state.
7. CLI prints plan or executor applies it.
8. Journal records every real action.
9. Stats and rollback use the journal.

## 5. Storage strategy
### Config
Use TOML for readability and easy edits.

### Journal
Use append-only JSONL for simplicity and crash tolerance, or SQLite if transactional queries are needed later. If SQLite is used, keep an export path to JSONL for easy auditing.

### Cache
Use a lightweight cache for hashes, prior classifications, and file fingerprints so repeated scans are faster.

## 6. Concurrency model
- Scanning can be parallelized over directories.
- Classification should be thread-safe.
- Watch events should be serialized through a work queue.
- Execution should be conservative: one move at a time or a controlled small batch.
- Rollback must lock the journal entry being restored.

## 7. Error handling model
Errors should be split into categories:
- Config errors
- Permission errors
- Filesystem errors
- Conflict errors
- AI backend errors
- Journal errors
- Watcher errors
- Internal invariants

Use structured error types with actionable messages. Never hide failure causes.

## 8. Fallback behavior
### AI backend unavailable
Use deterministic classification only.

### Watch backend unavailable
Disable watch mode but keep scan/plan/organize usable.

### Journal write fails
Abort the action if journal consistency is required.

### Conflict cannot be resolved safely
Skip the file and report it rather than risking overwrite.

### Destination path unavailable
Choose archive or fallback destination according to policy, otherwise leave unchanged.

## 9. Security and trust model
- Operate only on user-owned paths by default.
- Require explicit configuration to watch broader paths.
- Refuse hidden config and system paths unless explicitly allowed.
- Never execute file contents.
- Never trust filename alone for risky actions.

## 10. Packaging model
The release artifact should include:
- source code,
- binaries or build instructions,
- docs,
- install scripts,
- systemd unit,
- CI workflow,
- and a sample config.

## 11. Build/release flow
1. Implement the core crates.
2. Add integration tests.
3. Add documentation.
4. Run CI locally.
5. Package release tar/zip.
6. Tag the version.
7. Produce checksums.
8. Publish release notes.

## 12. Failure modes and architectural fallbacks
- If notify/inotify behaves poorly, fall back to periodic scan mode.
- If AI classification is unavailable, continue with rules.
- If journaling is partially corrupted, repair from the latest valid entry.
- If rollback cannot fully restore a move, report the exact subset restored and the blocked subset.
- If destination conflict resolution fails, quarantine the file to a conflict archive.

