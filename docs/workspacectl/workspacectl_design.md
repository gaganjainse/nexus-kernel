# workspacectl Design

## 1. User experience goals
The system should feel:
- safe,
- transparent,
- predictable,
- editable,
- and low-friction.

The user should always know:
- what was detected,
- why it was detected,
- where it will go,
- whether it is a dry-run or real action,
- and how to undo it.

## 2. CLI design principles
### Default behavior
- `plan` is the default mental model.
- `organize` should still be cautious and confirm broad changes.
- `watch` should be opt-in.
- `rollback` should always show exactly what it would restore.

### Output style
- Human-readable summary first.
- Detailed table or JSON when requested.
- Reason strings for every classification.
- Warning colors for risky operations.
- Clear success/failure counters.

## 3. Command design
### `workspacectl scan`
Purpose: inspect files and emit inventory.

Output should include:
- path,
- file type,
- category,
- rule match,
- confidence,
- size,
- modified time,
- and suggested destination.

Fallback: if metadata is incomplete, mark it as `unknown` rather than guessing.

### `workspacectl plan`
Purpose: generate a proposed move plan.

Output should include:
- source path,
- destination path,
- rule that triggered,
- conflict strategy,
- whether destination exists,
- whether action is risky,
- and whether user confirmation is needed.

Fallback: if destination cannot be determined, keep file unchanged and mark for review.

### `workspacectl organize`
Purpose: execute approved plan.

Behavior:
- must ask for confirmation when scope is broad or files are risky,
- must journal before/after each move,
- must stop if journal append fails,
- must skip files that are locked or unstable,
- must never overwrite.

Fallback: if a move fails, leave the source intact and record the failure.

### `workspacectl watch`
Purpose: run the daemon or connect to it.

Behavior:
- debounce filesystem events,
- wait for file stability,
- batch operations when safe,
- avoid moving active downloads,
- avoid acting on partially written archives.

Fallback: if watching fails, switch to periodic scan polling.

### `workspacectl rollback`
Purpose: undo one or more actions.

Behavior:
- show exact items that can be restored,
- support last-run rollback,
- support selected IDs,
- keep file conflicts visible,
- preserve newer files.

Fallback: if original location is occupied, restore to a conflict folder or suffixed path.

### `workspacectl doctor`
Purpose: validate readiness.

Checks should include:
- writable workspace path,
- permissions,
- availability of watch backend,
- journal path,
- config validity,
- hash tools,
- optional AI backend,
- systemd service status,
- and command dependencies.

### `workspacectl stats`
Purpose: summarize history.

Should report:
- total files scanned,
- categorized by type,
- moved by category,
- conflicts encountered,
- rollbacks performed,
- top destinations,
- recent events,
- and ignored file counts.

### `workspacectl learn`
Purpose: save user-approved patterns.

Behavior:
- only record confirmed or explicit choices,
- do not infer new permanent rules from a single ambiguous event unless configured,
- keep learned rules human-readable.

### `workspacectl clean`
Purpose: safe cleanup.

Should target:
- stale temp files,
- old downloads,
- known partial download extensions,
- archives selected by policy,
- and empty staging folders.

Fallback: never delete anything if ambiguity is high.

## 4. Workspace routing design
### AI files
- Models go to `Workspace/AI/Models/...`
- Datasets go to `Workspace/AI/Datasets/...`
- Experiments go to `Workspace/AI/Experiments/...`
- Benchmarks go to `Workspace/AI/Benchmarks/...`

### Projects
- Active repos go to `Workspace/Projects/Active/...`
- Archived repos go to `Workspace/Projects/Archive/...`
- Templates go to `Workspace/Projects/Templates/...`
- Experimental throwaways go to `Workspace/Projects/Playground/...`

### Learning and notes
- Docs and study material go to `Workspace/Learning/...`
- Personal notes go to `Workspace/Notes/...`

### Temporary and downloads
- Downloads remain a staging area until classification.
- Temp files are not aggressively moved unless the policy is safe.

## 5. Conflict strategy design
Supported strategies:
1. **Suffix**: append `-1`, `-2`, etc.
2. **Hash**: include short content hash in filename.
3. **Archive**: move to a conflict archive folder.
4. **Skip**: leave file untouched and alert.

Default should be safe and visible.

## 6. Learning design
The learning system should store:
- confirmed mappings,
- preferred destinations,
- manual overrides,
- ignored patterns,
- and category exceptions.

It should never auto-learn from:
- partial downloads,
- unstable files,
- unknown binary blobs,
- or one-off ambiguous cases without approval.

## 7. AI fallback design
If a local AI backend exists:
- only use it on ambiguous files,
- provide filename, extension, metadata, and nearby context,
- request a category recommendation plus explanation,
- treat output as advisory unless validated.

If AI backend does not exist:
- continue using rule-based classification only.

## 8. Exception handling design
### Locked files
Mark as skipped and retry later.

### Moving across filesystems
Use copy-then-verify-then-remove only if atomic rename is impossible, and only with explicit policy.

### Hidden folders
Ignore by default.

### Duplicate content
Detect via hash or size/hash pair and route to a duplicate policy.

### Empty folders
Leave them alone unless cleanup mode explicitly wants to remove them and confirmation is given.

## 9. How to proceed from the current point
The project was left at the scaffold/doc stage. The next practical design steps are:
1. Create the Rust workspace layout.
2. Implement config and journal crates first.
3. Implement scanner and classifier.
4. Implement planner with pure functions.
5. Implement executor and rollback.
6. Add watcher and daemon.
7. Add tests for every layer.
8. Add packaging and service files.
9. Add docs generated from the design and spec.
10. Run end-to-end validation on the Ubuntu workstation.

## 10. End-to-end flows
### First run flow
- initialize config,
- create workspace folders,
- run doctor,
- run scan on test paths,
- preview plan,
- confirm organization,
- validate journal,
- test rollback,
- then enable watch mode.

### Repeated run flow
- detect already-organized files,
- skip idempotent paths,
- only act on new or changed content,
- keep noise low.

### Recovery flow
- if the daemon crashes, restart,
- if journal repair is needed, rebuild from valid entries,
- if conflicts are unresolved, quarantine and report,
- if the user is unsure, default to plan-only.

