# workspacectl Specification

## 1. Scope
`workspacectl` is a Linux workspace/file organization system for a single user on Ubuntu. It provides a CLI and a daemon for scanning, planning, organizing, watching, rolling back, and reporting on file movements under a user-owned workspace. It is designed to be safe by default and reversible by design.

## 2. Supported commands
### `scan`
Inspect selected paths and emit a structured inventory.

### `plan`
Show proposed actions without modifying files.

### `organize`
Execute an approved plan.

### `watch`
Continuously watch configured folders and queue actions.

### `rollback`
Undo prior operations from the journal.

### `doctor`
Validate environment readiness, config, dependencies, permissions, and watch support.

### `stats`
Summarize scans, actions, destinations, ignored files, and conflicts.

### `learn`
Persist user-approved patterns or rules.

### `clean`
Handle safe cleanup tasks like stale temp files, archives, and downloads with explicit policy.

### `config`
Initialize, validate, print, or edit the configuration.

## 3. Default layout
The system should organize around `~/Workspace`:
- `AI/`
- `Projects/`
- `Learning/`
- `Scripts/`
- `Notes/`
- `Containers/`
- `Backups/`
- `Temp/`
- `Downloads/`
- `Assets/`

Subfolders for AI:
- `Models/`
  - `Ollama/`
  - `GGUF/`
  - `HuggingFace/`
  - `Embeddings/`
  - `Diffusion/`
- `Datasets/`
- `FineTuning/`
- `Benchmarks/`
- `Experiments/`
- `Training/`
- `Logs/`
- `Cache/`

Project structure:
- `Active/`
- `Archive/`
- `Playground/`
- `Templates/`

## 4. File and project categories
The classifier must support:
- Git repositories
- Rust projects
- Python projects
- Node/TypeScript projects
- Docker projects
- AI model files
- Datasets
- Screenshots
- PDFs
- Images
- Videos
- Audio
- ISOs
- Archives
- Documents
- Virtual environments
- Unknown/misc

## 5. Classification rules
Rules must be ordered:
1. Ignore rules.
2. Deterministic extension and path rules.
3. Project sentinel rules.
4. Size and metadata heuristics.
5. Optional AI classification for ambiguous cases.

Examples:
- `.git/` means Git repo.
- `Cargo.toml` means Rust project.
- `pyproject.toml`, `requirements.txt`, or `uv.lock` mean Python project.
- `package.json` or lockfiles mean Node project.
- `docker-compose.yml` or `Dockerfile` mean Docker project.
- `.gguf`, `.safetensors`, `.onnx`, `.pt`, `.pth`, `.ckpt` mean model assets.
- `.iso` means ISO.
- `.pdf` means document unless configured otherwise.

## 6. Safety rules
### Must never do automatically
- Move or edit hidden config folders such as `.config`, `.local`, `.ssh`, `.cargo`, `.rustup`, `.npm`, `.vscode`.
- Move system directories.
- Overwrite files.
- Delete user data without explicit confirmation.
- Act on partial downloads or temporary browser artifacts.

### Must do
- Default to dry-run.
- Require confirmation for destructive or broad actions.
- Create a journal entry for every actual change.
- Preserve rollback data.
- Handle conflicts by suffixing, archiving, or hashing.
- Log the reason for every move.

## 7. Rollback requirements
Every action that changes filesystem state must be reversible if possible.
Rollback must record:
- original path,
- destination path,
- timestamp,
- file hash or signature if useful,
- action ID,
- rule or reason,
- and conflict-handling details.

Rollback should:
- restore original location when safe,
- preserve newer conflicting files,
- fail loudly if recovery cannot be guaranteed,
- and support partial rollback if only some actions can be undone.

## 8. Watch mode requirements
The daemon must be able to:
- watch configured directories,
- debounce rapid events,
- avoid acting on incomplete writes,
- queue scan/plan operations,
- and process events idempotently.

Watch mode should not trigger on every low-level change. It should wait for stability and then classify.

## 9. Config requirements
Config must support:
- root workspace path,
- watched directories,
- ignore patterns,
- file category routing rules,
- conflict handling mode,
- confidence threshold for AI classification,
- dry-run default policy,
- journal location,
- cache location,
- and optional backend endpoint for local AI classification.

## 10. Output requirements
Command output should be:
- human-readable,
- terse by default,
- verbose when requested,
- and useful for scripting.

## 11. Acceptance criteria
The spec is met if:
- the CLI works as documented,
- the daemon can be installed and started,
- plans are deterministic,
- dry-run is safe,
- rollback works,
- tests cover core behaviors,
- and documentation explains the system clearly.

