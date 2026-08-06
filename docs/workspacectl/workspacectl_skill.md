# workspacectl Skill Guide

## 1. Purpose of this file
This file defines how the build skill should operate when creating or extending `workspacectl`. It is the operational contract for implementation work.

## 2. Required engineering posture
- Treat the project as production software.
- Prefer safety over automation.
- Prefer explicit rules over hidden heuristics.
- Prefer small, testable modules.
- Prefer reversible operations.
- Prefer clear logs and visible decisions.

## 3. Required tools and stack
### Language and build
- Rust stable toolchain.
- Cargo workspace.
- `clap` for CLI.
- `serde` for serialization.
- `thiserror` and `anyhow` for errors.
- `tracing` for logging.

### Filesystem and watch
- `walkdir` or equivalent for recursive scans.
- `notify` or direct inotify support for watch mode.
- Hashing library for fingerprinting.
- Regex support for rules.
- MIME or extension detection.

### Storage
- JSONL journal or SQLite journal.
- Config file in TOML.
- Cache for repeated scans and fingerprints.

### Packaging and devops
- systemd user service.
- GitHub Actions workflow.
- Installer script.
- Zip or tar release artifact.

## 4. Mandatory implementation habits
### Before coding
- Review the spec.
- Review the architecture.
- Confirm naming conventions.
- Confirm folder layout.
- Confirm fallback behavior.

### While coding
- Create one responsibility per module.
- Keep pure logic separate from I/O.
- Put decision logic in functions that are easy to test.
- Make all file mutation go through a narrow executor layer.
- Make journal writes explicit and observable.

### Before finishing a feature
- Add tests for normal cases.
- Add tests for failure cases.
- Add tests for rollback if state changes.
- Add docs for new commands or config.
- Verify dry-run output.

## 5. Build order
1. Workspace and crate skeleton.
2. Shared error/config/journal types.
3. Deterministic rules.
4. Scanner.
5. Planner.
6. Executor.
7. Rollback.
8. Watch daemon.
9. Optional AI classifier.
10. Tests.
11. Docs.
12. Packaging.
13. CI.
14. Release archive.

## 6. Required fallback behavior
If any major piece is blocked, continue with the lower-risk fallback.
- If AI is unavailable, use rules.
- If watch is unavailable, use polling.
- If SQLite is unavailable, use JSONL.
- If full packaging is not ready, still keep the source tree buildable.
- If one command is blocked, document the failure instead of hiding it.

## 7. Exception handling expectations
A skill implementation must explicitly cover:
- hidden files and folders,
- partial downloads,
- locked files,
- duplicate content,
- conflicts on destination,
- cross-filesystem moves,
- journal corruption,
- daemon restarts,
- and permission denials.

## 8. Testing expectations
Must test:
- rule matching,
- plan generation,
- destination routing,
- ignore rules,
- conflict resolution,
- journal append/restore,
- watch debouncing,
- config parsing,
- dry-run equivalence,
- idempotent reruns.

## 9. Release expectations
A finished implementation must include:
- build instructions,
- install instructions,
- sample config,
- service file,
- CI workflow,
- rollback notes,
- and a reproducible zip artifact.

## 10. Do not do
- Do not write a script pretending to be a daemon.
- Do not claim support for safety without journal-backed rollback.
- Do not ship without tests.
- Do not depend on cloud services.
- Do not move user data silently.
- Do not collapse categories into one generic bucket unless necessary.

