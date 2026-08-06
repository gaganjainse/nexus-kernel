# workspacectl Roadmap

## 1. Roadmap purpose
This roadmap turns the project from a documented idea into a complete release. It defines the build phases, gate checks, and fallback paths so the implementation can proceed safely without losing the project’s structure.

## 2. Current point in the project
The project is currently at the documentation-and-design stage. The required canvasses now exist for:
- vision,
- specification,
- architecture,
- design,
- skill contract,
- and execution planning.

The next step is to turn those documents into actual code, tests, packaging, and release artifacts.

## 3. Phase overview
### Phase 0 — Repo bootstrap
Goal: create a real Rust workspace and baseline project structure.

Deliverables:
- root `Cargo.toml`
- workspace crates
- `README.md`
- `LICENSE`
- `ATTRIBUTION.md`
- example config
- CI workflow skeleton
- install script skeleton

Success criteria:
- workspace builds at least one binary
- formatting succeeds
- docs exist
- baseline CI runs

Fallbacks:
- if the full workspace is too much at once, create a minimal root crate and expand immediately after
- if release packaging is blocked, still keep the repo buildable

### Phase 1 — Core domain and config
Goal: implement types, config loading, journal records, and safety primitives.

Deliverables:
- config parser
- error types
- file categories
- plan/action models
- journal entry types
- ignore patterns
- path safety helpers

Success criteria:
- config round-trip tests pass
- safety filters are deterministic
- journal schema is stable

Fallbacks:
- if TOML config becomes complex, split into nested tables
- if journal schema is too large, use JSONL first and keep schema evolution simple

### Phase 2 — Scanner and classifier
Goal: inspect files and classify them with deterministic rules.

Deliverables:
- recursive scan engine
- extension-based classifier
- project sentinel detection
- metadata extraction
- confidence scores
- rule explanations

Success criteria:
- scan results are stable and reproducible
- project types are detected correctly
- hidden/system/config paths are ignored

Fallbacks:
- if metadata cannot be read, classify conservatively as unknown
- if heuristics conflict, choose the safer category or unknown

### Phase 3 — Planner
Goal: convert scan results into safe, readable move plans.

Deliverables:
- destination router
- conflict resolver
- dry-run planner
- plan diff output
- policy engine

Success criteria:
- plans are deterministic
- no side effects during planning
- conflict handling never overwrites user files

Fallbacks:
- if destination cannot be determined, skip and report
- if conflicts cannot be resolved safely, quarantine or archive rather than overwrite

### Phase 4 — Executor and rollback
Goal: actually move files and reverse them safely.

Deliverables:
- atomic move executor
- journal writer
- rollback engine
- failure recovery logic
- action status reporting

Success criteria:
- every executed move is journaled
- rollback restores sample actions
- failures leave source files intact when possible

Fallbacks:
- if atomic rename is unavailable, use copy-verify-remove only when explicitly configured
- if rollback target is occupied, restore to a safe conflict path

### Phase 5 — Watch daemon
Goal: watch folders continuously and process new files safely.

Deliverables:
- `workspaced` daemon
- `watch` command
- debounce logic
- queueing and batching
- systemd user unit

Success criteria:
- event bursts are handled without duplicate moves
- daemon restarts cleanly
- watch mode can fall back to periodic scans

Fallbacks:
- if `notify` is unreliable, use polling mode
- if watcher cannot guarantee stability, mark item for later scan rather than acting immediately

### Phase 6 — Optional AI classifier
Goal: use a local AI backend only when deterministic rules are not enough.

Deliverables:
- backend adapter
- prompt builder
- response parser
- confidence gating
- offline fallback

Success criteria:
- AI is optional
- rule-only operation still works
- invalid model output does not break the system

Fallbacks:
- if the backend is unavailable, use rules only
- if the response is malformed, ignore it and keep the deterministic classification

### Phase 7 — Learning and stats
Goal: store user-approved patterns and summarize behavior.

Deliverables:
- learn command
- stats command
- learned rule persistence
- audit summaries

Success criteria:
- confirmed patterns are recorded in human-readable form
- stats reflect journal history
- learning does not auto-guess from ambiguous events

Fallbacks:
- if learned rules become too broad, require manual pruning
- if stats fail to read a damaged journal, report partial data instead of failing silently

### Phase 8 — Packaging and release
Goal: produce a distributable project.

Deliverables:
- installer script
- release zip
- checksums
- release notes
- CI workflow

Success criteria:
- fresh clone can build
- release artifacts are reproducible
- documentation matches behavior

Fallbacks:
- if full packaging is delayed, at least produce source + install script + docs
- if a binary artifact cannot be built in the environment, the project must still remain source-complete

## 4. Development order inside each phase
For every phase:
1. write the smallest useful module,
2. add tests for normal cases,
3. add tests for failure/fallback cases,
4. wire the module into the CLI,
5. document the behavior,
6. verify with local build/tests,
7. only then proceed.

## 5. Gate checks
The project should not move to the next phase unless:
- the current phase builds,
- tests pass,
- docs are updated,
- fallback behavior is verified,
- and the change does not weaken rollback or safety.

## 6. Exceptions and how to handle them
### Build breaks
- Stop.
- Fix the broken crate.
- Do not pile more features on top.

### Tests fail
- Reduce scope to the smallest failing case.
- Fix the failing module.
- Re-run the test suite.

### Watcher instability
- Use polling fallback.
- Reduce concurrency.
- Debounce more aggressively.

### Journal corruption
- Repair from the latest valid entry.
- Refuse risky rollback if integrity is uncertain.

### Ambiguous classification
- Prefer unknown over wrong.
- Use AI only if configured.

### Conflict on destination
- Never overwrite.
- Use suffix, hash, archive, or skip policy.

### Permission denials
- Skip and report.
- Do not escalate into dangerous behavior.

## 7. How to know the roadmap is complete
The roadmap is complete only when the project has reached a releasable state with:
- working CLI,
- daemon,
- config,
- watch mode,
- rollback,
- stats,
- tests,
- docs,
- CI,
- install script,
- and a final release archive.

