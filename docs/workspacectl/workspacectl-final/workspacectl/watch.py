from __future__ import annotations
import os
import time
import subprocess
from pathlib import Path
from typing import Dict, Tuple, Iterable

from .organizer import plan, apply
from .config import expand

def snapshot(paths: Iterable[Path]) -> Dict[str, Tuple[int, int]]:
    snap: Dict[str, Tuple[int, int]] = {}
    for root in paths:
        root = expand(root)
        if not root.exists():
            continue
        for cur, dirs, files in os.walk(root):
            curp = Path(cur)
            dirs[:] = [d for d in dirs if not d.startswith(".") and d not in {"node_modules","target","__pycache__"}]
            for name in files:
                p = curp / name
                try:
                    st = p.stat()
                except FileNotFoundError:
                    continue
                snap[str(p)] = (st.st_size, int(st.st_mtime))
    return snap

def has_inotifywait() -> bool:
    from shutil import which
    return which("inotifywait") is not None

def watch_with_inotify(paths: Iterable[Path], cfg: dict) -> None:
    cmd = [
        "inotifywait", "-m", "-r",
        "-e", "close_write", "-e", "moved_to", "-e", "create",
        "--format", "%w%f"
    ]
    cmd.extend(str(expand(p)) for p in paths)
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        assert proc.stdout is not None
        for line in proc.stdout:
            p = line.strip()
            if not p:
                continue
            path = Path(p)
            if not path.exists():
                continue
            actions = plan([path], cfg)
            if actions:
                apply(actions, cfg, dry_run=bool(cfg.get("dry_run", False)))
                for src, dst, reason in actions:
                    print(f"{src} -> {dst} [{reason}]")
    finally:
        proc.terminate()

def watch_poll(paths: Iterable[Path], cfg: dict, interval: int = 15) -> None:
    prev = snapshot(paths)
    print("watching with polling; press Ctrl+C to stop")
    while True:
        time.sleep(interval)
        cur = snapshot(paths)
        added = []
        for p, meta in cur.items():
            if p not in prev or prev[p] != meta:
                added.append(Path(p))
        prev = cur
        if not added:
            continue
        actions = plan(added, cfg)
        if not actions:
            continue
        apply(actions, cfg, dry_run=bool(cfg.get("dry_run", False)))
        for src, dst, reason in actions:
            print(f"{src} -> {dst} [{reason}]")
