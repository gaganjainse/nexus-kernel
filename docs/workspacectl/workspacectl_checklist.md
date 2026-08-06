# workspacectl Master Checklist

## 1. Use of this checklist
This checklist is the operational control sheet for building `workspacectl`. It should be followed in order. Each item is a concrete step, not a vague intention.

## 2. Pre-flight checklist
Before writing code:
- [ ] Read `vision.md`
- [ ] Read `spec.md`
- [ ] Read `architecture.md`
- [ ] Read `design.md`
- [ ] Read `skill.md`
- [ ] Read `agent.md`
- [ ] Confirm the workspace layout is final
- [ ] Confirm the build environment (Ubuntu, Rust, Cargo, Git)
- [ ] Confirm the packaging target (zip release)

### Pre-flight exceptions
- If any doc conflicts with another, stop and resolve the conflict before implementation.
- If the environment cannot build Rust, stop and fix the toolchain first.
- If the release target changes, update docs before coding.

## 3. Repository bootstrap checklist
- [ ] Create root repository
- [ ] Add Cargo workspace layout
- [ ] Add crates for CLI, daemon, core, config, journal, watch, AI, tests, packaging
- [ ] Add `README.md`
- [ ] Add `LICENSE`
- [ ] Add `ATTRIBUTION.md`
- [ ] Add sample config file
- [ ] Add CI workflow
- [ ] Add install script
- [ ] Add systemd user service
- [ ] Add release notes template

### Bootstrap fallback checklist
- [ ] If multiple crates are too much at once, create a minimal workspace and expand in order
- [ ] If CI cannot yet build the full tree, keep placeholder jobs but ensure they fail loudly when implementation is missing

## 4. Core implementation checklist
### Config and types
- [ ] Define config schema
- [ ] Define category enums
- [ ] Define action/plan structs
- [ ] Define journal entry structs
- [ ] Define ignore rules
- [ ] Add serialization tests

### Scanner
- [ ] Recursive walk implemented
- [ ] Metadata extraction implemented
- [ ] Hidden/config/system path filters implemented
- [ ] Partial download filters implemented
- [ ] Scan output stable and readable

### Classifier
- [ ] Extension rules implemented
- [ ] Project sentinel rules implemented
- [ ] Heuristics implemented
- [ ] Confidence scoring implemented
- [ ] Explanations emitted

### Planner
- [ ] Destination routing implemented
- [ ] Conflict resolution implemented
- [ ] Dry-run output implemented
- [ ] Idempotence guaranteed

### Executor
- [ ] Atomic move path implemented
- [ ] Journal written before/after each real action
- [ ] No-overwrite guarantee enforced
- [ ] Failure leaves source intact when safe

### Rollback
- [ ] Rollback journal parse implemented
- [ ] Single action rollback implemented
- [ ] Batch rollback implemented
- [ ] Conflict rollback path implemented
- [ ] Partial rollback reporting implemented

### Watch daemon
- [ ] Filesystem watcher implemented
- [ ] Debounce implemented
- [ ] Queue implemented
- [ ] Daemon foreground mode implemented
- [ ] systemd unit validated

### Optional AI
- [ ] Local backend adapter implemented
- [ ] Ambiguous-case prompt builder implemented
- [ ] Response parser hardened
- [ ] Rules-only fallback preserved

## 5. Testing checklist
- [ ] Config parse/roundtrip tests
- [ ] Rule matching tests
- [ ] Path ignore tests
- [ ] Classification tests
- [ ] Planner determinism tests
- [ ] Dry-run tests
- [ ] Conflict tests
- [ ] Journal append/read tests
- [ ] Rollback tests
- [ ] Watch debounce tests
- [ ] Idempotence tests
- [ ] Empty-directory tests
- [ ] Permission-denied tests
- [ ] AI fallback tests

### Testing exceptions
- If a test is flaky, reduce concurrency or isolate filesystem dependencies.
- If a test needs system integration, keep a non-destructive version in unit tests and a separate integration test.
- If a fallback is untestable in CI, document a manual verification step.

## 6. Documentation checklist
- [ ] README usage section
- [ ] README install section
- [ ] README config section
- [ ] README rollback section
- [ ] README safety section
- [ ] README troubleshooting section
- [ ] Config example documented
- [ ] systemd unit documented
- [ ] CLI help text accurate
- [ ] Attribution notes complete

## 7. Packaging checklist
- [ ] Build release binary
- [ ] Include Cargo.lock
- [ ] Include sample config
- [ ] Include docs
- [ ] Include systemd service
- [ ] Include install script
- [ ] Include checksums or release notes
- [ ] Zip archive generated

### Packaging fallback checklist
- [ ] If binaries cannot be packaged, package source plus build instructions
- [ ] If zip creation fails, create tarball as a temporary fallback and explain why

## 8. Final validation checklist
- [ ] Fresh clone builds
- [ ] Fresh config validates
- [ ] `doctor` passes or reports only known non-blocking warnings
- [ ] `scan` works on sample files
- [ ] `plan` is dry-run safe
- [ ] `organize` journals correctly
- [ ] `rollback` restores sample moves
- [ ] `watch` handles a new file event
- [ ] `stats` summarizes journal history
- [ ] Final archive exists
- [ ] Release notes mention known limitations

## 9. Stop/alert checklist
Stop immediately and fix the issue if:
- [ ] Any real move can overwrite data
- [ ] Rollback cannot restore a successful move
- [ ] Hidden config folders are being touched
- [ ] System directories are in scope
- [ ] AI output is accepted without validation in a risky case
- [ ] Tests are failing in core safety behavior
- [ ] Docs and implementation diverge in a user-facing command

## 10. Completion definition
The checklist is complete only when every mandatory item is checked and the resulting project is a reproducible, safe, release-quality toolchain rather than a scaffold.

