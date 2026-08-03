# NexusAOS v2 — Implementation Plan

## Purpose

This document turns the architecture into a build order. It is written so that a coder can follow it step by step without needing a new reasoner at every stage.

The plan is bottom-up:
1. build the smallest safe pieces first,
2. prove each piece with tests,
3. connect pieces only after the lower layer is stable,
4. keep the kernel small,
5. keep models and tools behind contracts.

## Architecture additions to include before coding

The architecture is strong already. One important missing piece is a **Context Manager**.

### Why the Context Manager is needed

Gemma, Qwen, and the vision model will not always need the same context size. The system must choose a context budget based on task size, available RAM, VRAM, and queue pressure.

The Context Manager should sit in the kernel control plane and do four jobs:
- estimate tokens needed,
- choose a safe context limit,
- ask the Resource Monitor if the machine can afford it,
- clamp the request to policy and model capability.

### Other missing pieces to keep explicit

- **Resource Monitor**: reports RAM, VRAM, disk, queue depth, and backend health.
- **Budget Manager**: enforces hard limits for tasks, models, and tool calls.
- **Task Router**: chooses planner, coder, or vision based on task type.
- **Read model / projection layer**: fast current-state views built from events.
- **Rollback / checkpoint service**: restores safe state after failure.

## Working rules

- Do not jump to UI polish first.
- Do not keep three heavy models loaded at once.
- Do not write directly to shared state from workers.
- Do not let the model decide policy.
- Do not let tools bypass the kernel.
- Do one layer at a time.
- Every change must have tests.
- Every step must have a clear done condition.

## Final target architecture

```text
NexusAOS/
├── Cargo.toml
├── README.md
├── configs/
│   ├── default.toml
│   └── agents/
├── docs/
│   ├── architecture.md
│   ├── manifest-spec.md
│   └── event-spec.md
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── task.rs
│   ├── state.rs
│   ├── events.rs
│   ├── manifest.rs
│   ├── capability.rs
│   ├── policy.rs
│   ├── context.rs
│   ├── resource.rs
│   ├── router.rs
│   ├── model/
│   │   ├── mod.rs
│   │   ├── provider.rs
│   │   ├── registry.rs
│   │   ├── gemma.rs
│   │   ├── coder.rs
│   │   └── vision.rs
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── broker.rs
│   │   ├── filesystem.rs
│   │   ├── git.rs
│   │   ├── terminal.rs
│   │   └── docker.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── event_store.rs
│   │   ├── snapshot.rs
│   │   ├── artifact_store.rs
│   │   └── projection.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── kernel.rs
│   │   ├── scheduler.rs
│   │   ├── replay.rs
│   │   └── rollback.rs
│   └── cli/
│       ├── mod.rs
│       ├── doctor.rs
│       ├── init.rs
│       ├── status.rs
│       ├── plan.rs
│       ├── run.rs
│       └── replay.rs
├── tests/
│   ├── integration/
│   └── fixtures/
└── scripts/
    ├── dev.sh
    └── test.sh
```

## Bottom-up build order

## Phase 0 — repository and safety rails

### Goal

Create a clean Rust workspace, enforce style, and make the project easy to work on.

### Steps

1. Create the repository root.
2. Add `Cargo.toml` with workspace or single-crate layout.
3. Add `rustfmt.toml`.
4. Add `clippy` settings.
5. Add a `README.md` with project purpose.
6. Add `docs/architecture.md` with the design summary.
7. Add the initial folder structure.
8. Add basic CI or local test scripts.

### Done when

- `cargo test` runs.
- `cargo fmt --check` runs.
- `cargo clippy` runs.
- repository layout matches the tree above.

---

## Phase 1 — domain types first

### Goal

Create the stable language of the system before building behavior.

### Files

```text
src/
├── lib.rs
├── config.rs
├── task.rs
├── state.rs
├── events.rs
├── manifest.rs
├── capability.rs
└── policy.rs
```

### What to build

#### `src/config.rs`
- App config.
- Backend choice.
- Policy flags.
- Resource budgets.
- Paths.

#### `src/task.rs`
- Task ID.
- Task request.
- Task outcome.
- Task priority.
- Task input types.

#### `src/state.rs`
- Task state machine.
- Kernel state.
- Manifest activation state.
- Current task record.

#### `src/events.rs`
- Event ID.
- Event kind.
- Event payload.
- Event metadata.
- event append contract.

#### `src/manifest.rs`
- Manifest version.
- Manifest state.
- Manifest validation.
- Agent role selection.

#### `src/capability.rs`
- Capability.
- Capability lease.
- expiry and scope.
- revocation rules.

#### `src/policy.rs`
- policy engine types.
- approval decision.
- violation types.
- deny-by-default rules.

### Done when

- every type compiles,
- every type has serialization if needed,
- unit tests confirm default values and state names,
- no runtime logic is mixed into the type files.

---

## Phase 2 — event store and replay spine

### Goal

Make the event log the source of truth.

### Files

```text
src/storage/
├── mod.rs
├── event_store.rs
├── snapshot.rs
├── artifact_store.rs
└── projection.rs
```

### What to build

- append-only event store,
- snapshot writing,
- snapshot loading,
- projection rebuild,
- task history query,
- event checksums,
- idempotency keys,
- durable write path.

### Rules

- One event append must equal one factual change.
- A projection can be rebuilt from events.
- A snapshot must never be treated as the truth.

### Done when

- a task can be written,
- events can be read back,
- a projection can be rebuilt,
- a restart does not lose event history.

---

## Phase 3 — kernel runtime and scheduler

### Goal

Build the trusted control plane that owns task execution.

### Files

```text
src/runtime/
├── mod.rs
├── kernel.rs
├── scheduler.rs
├── replay.rs
└── rollback.rs
```

### What to build

- kernel entrypoint,
- task admission,
- task state transitions,
- scheduler queue,
- queue depth enforcement,
- rollback path,
- replay path,
- kernel status reporting.

### Kernel responsibilities

- accept or reject work,
- classify tasks,
- issue leases,
- call the router,
- request model generation,
- authorize tools,
- persist outcomes,
- update projections.

### Done when

- kernel can accept a request,
- state transitions are valid,
- kernel can reject unsafe or oversized tasks,
- restart can replay state.

---

## Phase 4 — resource control and dynamic context

### Goal

Stop the system from guessing memory use.

### Files

```text
src/
├── context.rs
└── resource.rs
```

### What to build

#### `src/resource.rs`
- RAM monitor.
- VRAM monitor.
- disk monitor.
- queue monitor.
- backend health monitor.

#### `src/context.rs`
- context estimator.
- context clamping.
- task complexity scoring.
- model capability matching.
- context growth policy.

### The Context Manager must do

- inspect the task,
- estimate token budget,
- inspect system pressure,
- choose safe context size,
- refuse growth when memory is too tight,
- prefer smaller contexts unless the task needs more.

### Example policy

- simple question → 8K
- small code edit → 16K
- feature work → 32K
- architecture / repo reasoning → 64K
- only when absolutely necessary → higher, but still clamped by budget and model max

### Done when

- every request gets a context budget before inference,
- context budget is visible in logs,
- oversized budgets are clamped,
- pressure causes refusal or downgrade instead of thrash.

---

## Phase 5 — model provider abstraction and routing

### Goal

Make the system work with other models later without changing the kernel.

### Files

```text
src/model/
├── mod.rs
├── provider.rs
├── registry.rs
├── gemma.rs
├── coder.rs
└── vision.rs
src/router.rs
```

### What to build

- provider trait,
- provider registry,
- health checks,
- load/unload lifecycle,
- capability declarations,
- role-to-model routing,
- fallback selection,
- cancellation handling.

### Roles

- Planner → Gemma Agentic
- Coder → Qwen3-Coder
- Vision → Qwen3.5 vision model

### Router rules

- planning goes to planner first,
- implementation goes to coder,
- screenshots and documents go to vision,
- ambiguous tasks go to planner first,
- the router does not execute tools directly.

### Done when

- providers can be swapped by config,
- a role can be resolved to a provider,
- the kernel can ask for health and capability info,
- one heavy model can be loaded and unloaded cleanly.

---

## Phase 6 — tools and tool broker

### Goal

Let models act only through approved tools.

### Files

```text
src/tools/
├── mod.rs
├── broker.rs
├── filesystem.rs
├── git.rs
├── terminal.rs
└── docker.rs
```

### What to build

- tool registry,
- tool request schema,
- tool result schema,
- capability checks,
- timeouts,
- sandbox launch,
- tool logging.

### Tool rules

- all tool calls go through the broker,
- all tool calls are logged as events,
- destructive actions need approval,
- tool outputs are normalized,
- tools can fail without crashing the kernel.

### Done when

- read-only filesystem access works,
- git status and diff work,
- terminal execution is gated,
- tool failures are recoverable.

---

## Phase 7 — compiler pipeline for Genome → IR → Manifest

### Goal

Create the declarative agent definition path.

### Files

```text
docs/
├── manifest-spec.md
└── event-spec.md
src/
└── manifest.rs
```

### What to build

- genome schema,
- IR schema,
- optimizer rules,
- manifest validation,
- manifest signing or integrity checking,
- manifest activation and versioning.

### Manifest lifecycle

- draft
- validated
- signed
- active
- superseded
- retired

### Done when

- a genome can be compiled to a manifest,
- the manifest can be validated,
- invalid capabilities are rejected,
- a manifest version can be stored and reloaded.

---

## Phase 8 — multi-model workflow support

### Goal

Make the system usable with the three chosen models without re-planning each time.

### Model usage plan

#### Planner model
Use for:
- architecture
- decomposition
- review
- trade-offs
- failure analysis

#### Coder model
Use for:
- implementation
- refactoring
- debugging
- test generation
- file edits

#### Vision model
Use for:
- screenshots
- UI understanding
- documents
- diagrams
- OCR-like reading where needed

### Step-by-step workflow

1. Planner reads the task and proposes a plan.
2. Kernel turns the plan into a task with a budget.
3. Coder implements one small change set.
4. Kernel records tool calls and file diffs.
5. Planner reviews the result.
6. Vision is used only when an image, UI, or document is involved.
7. Kernel stores the final event chain.

### Done when

- the same task can be routed consistently,
- the role boundaries remain separate,
- the kernel can switch models without changing task semantics.

---

## Phase 9 — CLI and operator commands

### Goal

Make the system inspectable and usable from the terminal.

### Files

```text
src/cli/
├── mod.rs
├── doctor.rs
├── init.rs
├── status.rs
├── plan.rs
├── run.rs
└── replay.rs
src/main.rs
```

### Commands

- `nexusaos init`
- `nexusaos doctor`
- `nexusaos status`
- `nexusaos plan <file>`
- `nexusaos run <task>`
- `nexusaos replay <task-id>`

### Done when

- the CLI is clean,
- each command maps to one kernel action,
- status is readable,
- errors are helpful.

---

## Phase 10 — tests, hardening, and release readiness

### Goal

Make the system safe enough to trust.

### Test groups

- unit tests for every module,
- integration tests for kernel flow,
- event replay tests,
- rollback tests,
- permission denial tests,
- memory-pressure tests,
- provider failure tests,
- tool failure tests,
- manifest validation tests.

### Hardening checklist

- no silent writes,
- no unlogged state changes,
- no tool bypass,
- no model bypass,
- no uncaught task panic,
- no schema drift without versioning,
- no mixed responsibility files.

### Done when

- the system survives restart,
- every state change is traceable,
- every failure leaves the system recoverable,
- the code layout matches the architecture.

## Multi-model prompting workflow

This is how to work with other models cleanly:

### Planner prompt
Use the planner for:
- design
- architecture review
- trade-offs
- step ordering
- safety analysis

### Coder prompt
Use the coder for:
- file creation
- file edits
- implementation
- tests
- refactors

### Reviewer prompt
Use the reviewer for:
- correctness checks
- style checks
- architecture drift checks
- edge cases

### Vision prompt
Use the vision model for:
- screenshots
- PDFs
- diagrams
- UI inspection

## How to hand this to the coder

Give the coder only the current phase and the files for that phase.
Do not ask the coder to build the whole system at once.
Always ask for:
- one phase,
- one file tree,
- one clear done condition,
- one test target.

## Final order of work

1. Build domain types.
2. Build event store.
3. Build kernel.
4. Build resource and context managers.
5. Build model router and providers.
6. Build tools.
7. Build manifest compiler.
8. Build CLI.
9. Add tests.
10. Harden and ship.

