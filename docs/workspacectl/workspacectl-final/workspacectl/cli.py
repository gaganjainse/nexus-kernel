from __future__ import annotations
import argparse
import json
import os
import shutil
import sys
from pathlib import Path

from .config import load_config, save_config, expand
from .organizer import plan, apply, adopt_home, summary
from .watch import watch_poll, watch_with_inotify, has_inotifywait
from .journal import read_all, clear

def fmt_actions(actions):
    if not actions:
        print("No actions.")
        return
    for src, dst, reason in actions:
        print(f"{src}  ->  {dst}    [{reason}]")
    s = summary(actions)
    print(f"\nTotal: {s['total']}")

def cmd_init(args):
    cfg = load_config()
    save_config(cfg)
    ws = expand(cfg["workspace_root"])
    structure = [
        ws / "AI" / "Models",
        ws / "AI" / "Datasets",
        ws / "AI" / "Training",
        ws / "AI" / "FineTuning",
        ws / "AI" / "Benchmarks",
        ws / "AI" / "Embeddings",
        ws / "AI" / "Experiments",
        ws / "Projects" / "Active",
        ws / "Projects" / "Archive",
        ws / "Projects" / "Playground",
        ws / "Projects" / "Templates",
        ws / "Learning" / "Python",
        ws / "Learning" / "Rust",
        ws / "Learning" / "Linux",
        ws / "Learning" / "AI",
        ws / "Scripts",
        ws / "Containers",
        ws / "Notes",
        ws / "Temp",
        ws / "Temp" / "Quarantine",
        ws / "Temp" / "Archives",
        ws / "Temp" / "ISOs",
        ws / "Assets" / "Images",
        ws / "Assets" / "Audio",
        ws / "Assets" / "Video",
        ws / "Backups",
    ]
    for p in structure:
        p.mkdir(parents=True, exist_ok=True)
    print(f"Config written: {save_config(cfg)}")
    print(f"Workspace initialized: {ws}")

def resolve_paths(args, cfg):
    if args.paths:
        return [expand(p) for p in args.paths]
    return [expand(p) for p in cfg["watch_paths"]]

def cmd_plan(args):
    cfg = load_config()
    paths = resolve_paths(args, cfg)
    acts = plan(paths, cfg)
    fmt_actions(acts)

def cmd_organize(args):
    cfg = load_config()
    if args.no_dry_run:
        cfg["dry_run"] = False
    paths = resolve_paths(args, cfg)
    acts = plan(paths, cfg)
    if not acts:
        print("No actions.")
        return
    if args.json:
        print(json.dumps([{"src": str(s), "dst": str(d), "reason": r} for s, d, r in acts], indent=2))
        return
    fmt_actions(acts)
    if cfg.get("dry_run", False):
        ans = input("\nApply these changes? [y/N] ").strip().lower()
        if ans != "y":
            print("Cancelled.")
            return
    apply(acts, cfg, dry_run=False)
    print("Done.")

def cmd_watch(args):
    cfg = load_config()
    if args.no_dry_run:
        cfg["dry_run"] = False
    paths = resolve_paths(args, cfg)
    if has_inotifywait():
        print("Using inotifywait.")
        watch_with_inotify(paths, cfg)
    else:
        print("inotifywait not found; using polling watcher.")
        watch_poll(paths, cfg, interval=args.interval)

def cmd_rollback(args):
    cfg = load_config()
    entries = read_all(cfg)
    if not entries:
        print("No journal entries found.")
        return
    moved = 0
    for entry in reversed(entries):
        if entry.op != "move":
            continue
        src = Path(entry.src)
        dst = Path(entry.dst)
        if dst.exists() and not src.exists():
            src.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(dst), str(src))
            moved += 1
            print(f"{dst} -> {src}")
    print(f"Rolled back {moved} move(s).")
    if moved:
        clear(cfg)

def cmd_doctor(args):
    cfg = load_config()
    print("Config:")
    print(json.dumps(cfg, indent=2))
    print("\nTools:")
    for tool in ["inotifywait", "python3", "git", "cargo", "ollama", "docker"]:
        print(f"  {tool:12} {'yes' if shutil.which(tool) else 'no'}")
    print(f"\nJournal: {Path(cfg['journal_dir']).expanduser()}")
    print(f"Workspace: {Path(cfg['workspace_root']).expanduser()}")

def cmd_adopt(args):
    cfg = load_config()
    if args.no_dry_run:
        cfg["dry_run"] = False
    acts = adopt_home(cfg, dry_run=cfg.get("dry_run", False))
    if not acts:
        print("No home folders matched the allowlist.")
        return
    fmt_actions(acts)
    if cfg.get("dry_run", False):
        ans = input("\nApply these changes? [y/N] ").strip().lower()
        if ans != "y":
            print("Cancelled.")
            return
        adopt_home(cfg, dry_run=False)
    print("Done.")

def build_parser():
    p = argparse.ArgumentParser(prog="workspacectl")
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser("init", help="create config and workspace structure")
    sp.set_defaults(func=cmd_init)

    sp = sub.add_parser("plan", help="show planned moves")
    sp.add_argument("paths", nargs="*", help="paths to scan (defaults to config watch paths)")
    sp.set_defaults(func=cmd_plan)

    sp = sub.add_parser("organize", help="apply planned moves")
    sp.add_argument("paths", nargs="*", help="paths to scan (defaults to config watch paths)")
    sp.add_argument("--no-dry-run", action="store_true", help="apply immediately")
    sp.add_argument("--json", action="store_true", help="output JSON plan only")
    sp.set_defaults(func=cmd_organize)

    sp = sub.add_parser("watch", help="watch and organize incoming files")
    sp.add_argument("paths", nargs="*", help="paths to watch (defaults to config watch paths)")
    sp.add_argument("--interval", type=int, default=15, help="poll interval seconds when inotifywait is absent")
    sp.add_argument("--no-dry-run", action="store_true", help="apply immediately")
    sp.set_defaults(func=cmd_watch)

    sp = sub.add_parser("rollback", help="undo moves recorded in journal")
    sp.set_defaults(func=cmd_rollback)

    sp = sub.add_parser("doctor", help="show config and tool status")
    sp.set_defaults(func=cmd_doctor)

    sp = sub.add_parser("adopt-home", help="move selected loose top-level folders into Workspace")
    sp.add_argument("--no-dry-run", action="store_true", help="apply immediately")
    sp.set_defaults(func=cmd_adopt)

    return p

def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    args.func(args)

if __name__ == "__main__":
    main()
