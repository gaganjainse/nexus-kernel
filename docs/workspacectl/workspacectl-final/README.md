# workspacectl

`workspacectl` is a clean-room, local file organizer for Linux workstations.

It is inspired by three open-source ideas:

- `inotify-tools`, which exposes Linux filesystem events to scripts and shells. The Linux kernel `inotify` API is the event mechanism underneath. citeturn538035search1turn578745view0turn280784search1
- rule-based file organizers that support dry-run, undo, logs, and configurable categories. citeturn578745view1turn578745view2
- MCP-based local file organization systems that add directory security and project detection. citeturn578745view2

This project is **original code**, not a literal merge of upstream repositories.

## What it does

- plans moves before making changes
- organizes files into a workspace tree
- detects common project roots
- keeps a rollback journal
- watches folders continuously with a polling loop
- optionally uses `inotifywait` if it is installed

## Quick start

```bash
chmod +x workspacectl.sh
./workspacectl.sh init
./workspacectl.sh plan ~/Downloads
./workspacectl.sh organize ~/Downloads
./workspacectl.sh watch
```

## Typical layout

```text
~/Workspace
├── AI
│   ├── Models
│   ├── Datasets
│   ├── Training
│   ├── FineTuning
│   ├── Benchmarks
│   ├── Embeddings
│   └── Experiments
├── Projects
│   ├── Active
│   ├── Archive
│   ├── Playground
│   └── Templates
├── Learning
├── Scripts
├── Containers
├── Notes
├── Temp
├── Assets
└── Backups
```

## Commands

- `init` — create the workspace tree
- `plan [paths...]` — show what would move where
- `organize [paths...]` — apply moves
- `watch [paths...]` — watch and organize new arrivals
- `rollback` — undo the most recent operations recorded in the journal
- `doctor` — show config and tool status
- `adopt-home` — move selected loose top-level folders into `~/Workspace`

## Config

Configuration is stored at:

```text
~/.config/workspacectl/config.json
```

Run `workspacectl init` to create it.

## Notes

- Hidden config directories are left alone.
- Standard Linux user folders are left alone.
- A dry-run is used when possible before any file moves.
- The rollback journal is appended to per move.

## License compatibility note

`inotify-tools` is GPL-2.0-only upstream. Direct code copying would require careful license review. This project avoids upstream code copying and implements the behavior independently. citeturn578745view0turn280784search1
