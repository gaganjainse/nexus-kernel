# workspacectl Vision

## 1. What this project is
`workspacectl` is a safe, local-first workspace and file organization system for Ubuntu Linux. It is designed for a single-user workstation that accumulates projects, datasets, downloads, screenshots, models, notes, and temporary files over time. The system consists of a command-line interface (`workspacectl`) and a background daemon (`workspaced`) that can scan, classify, organize, watch, and roll back file moves.

The core idea is simple:
- keep the user’s workspace centered around `~/Workspace`,
- make file organization predictable,
- never lose user data,
- make every move reversible,
- support both manual and automatic organization,
- and work offline by default.

## 2. Why it exists
Most desktop file organizers are either too manual, too risky, too platform-specific, or too opaque. This project exists to solve the practical problems of an engineering workstation:
- project folders spread everywhere,
- downloads piling up,
- screenshots mixed with documents,
- models and datasets scattered across multiple paths,
- no audit trail for moves,
- no reliable undo,
- and no structured place to keep AI engineering assets.

This project should feel like a serious filesystem control plane, not a toy file mover.

## 3. Product promise
The project should guarantee the following experience:
1. The user can run a dry-run plan before any real move.
2. The user can approve or reject a plan.
3. Every move is logged.
4. Every move can be rolled back.
5. The daemon can watch folders continuously.
6. The system can classify common file and project types with deterministic rules first.
7. Optional AI classification is only used for ambiguous cases.
8. The system never overwrites files silently.
9. The system never touches hidden config folders or system directories unless explicitly told to.
10. The system can be rebuilt from source and reinstalled reliably.

## 4. Success criteria
A release is successful only if all of the following are true:
- `workspacectl` builds cleanly.
- `workspaced` runs as a service.
- `scan`, `plan`, `organize`, `watch`, `rollback`, `doctor`, `stats`, `learn`, and `clean` exist and behave as documented.
- Tests pass.
- The project includes docs, CI, and packaging.
- A dry-run and rollback can be demonstrated end to end.
- The workspace layout is stable and repeatable.

## 5. Non-goals
This project is **not**:
- a full desktop shell,
- a cloud sync tool,
- a backup system,
- a content editor,
- a general-purpose AI agent platform,
- a hidden background sorter that acts without trace,
- or a system that reorganizes every folder on the machine indiscriminately.

## 6. Product principles
### Safety over automation
The system should prefer asking or planning over acting when uncertainty is high.

### Determinism over guessing
Rules should be deterministic when possible.

### Reversibility over cleverness
If a move cannot be rolled back reliably, it should not happen automatically.

### Local-first
The project must work without internet or cloud services.

### Clear user consent
Destructive or broad actions should be confirmed explicitly.

### Minimal surprise
The user should understand why each file is going somewhere.

## 7. User outcome
After using the project, the user should have:
- a clean `~/Workspace` hierarchy,
- a consistent place for projects and AI assets,
- a searchable action log,
- confidence that accidental moves are undoable,
- and a tool they can trust on a primary workstation.

## 8. Definition of done for the vision layer
The vision is satisfied only when the project is no longer just an organizer, but a reliable workstation management system with:
- explicit policy,
- safe execution,
- journaling,
- watching,
- rollback,
- and maintainable code.

