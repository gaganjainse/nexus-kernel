from __future__ import annotations
import os
import time
from pathlib import Path
from typing import Iterable, List, Tuple

from .rules import classify_file, classify_dir, Decision
from .utils import move_path, ensure_dir
from .journal import JournalEntry, append

def should_skip(path: Path, cfg: dict) -> bool:
    parts = set(path.parts)
    return any(skip in parts for skip in cfg.get("skip_dirs", []))

def iter_entries(root: Path) -> Iterable[Path]:
    if root.is_file():
        yield root
        return
    for cur, dirs, files in os.walk(root):
        curp = Path(cur)
        dirs[:] = [d for d in dirs if not d.startswith(".") and d not in {"node_modules", "target", "__pycache__"}]
        for d in dirs:
            yield curp / d
        for f in files:
            yield curp / f

def classify(path: Path, cfg: dict) -> Decision:
    if path.is_dir():
        return classify_dir(path, cfg)
    return classify_file(path, cfg)

def plan(paths: Iterable[Path], cfg: dict) -> List[Tuple[Path, Path, str]]:
    actions: List[Tuple[Path, Path, str]] = []
    for root in paths:
        if not root.exists():
            continue
        if root.is_file():
            dec = classify(root, cfg)
            if dec.destination is not None:
                actions.append((root, dec.destination / root.name, dec.reason))
        else:
            for p in iter_entries(root):
                if should_skip(p, cfg):
                    continue
                if p.is_dir():
                    dec = classify(p, cfg)
                    if dec.destination is not None and dec.destination != p:
                        actions.append((p, dec.destination, dec.reason))
                else:
                    dec = classify(p, cfg)
                    if dec.destination is not None:
                        actions.append((p, dec.destination / p.name, dec.reason))
    seen = set()
    uniq = []
    for a in actions:
        key = (str(a[0]), str(a[1]))
        if key not in seen:
            uniq.append(a)
            seen.add(key)
    return uniq

def apply(actions: Iterable[Tuple[Path, Path, str]], cfg: dict, *, dry_run: bool = False) -> List[Tuple[Path, Path, str]]:
    results: List[Tuple[Path, Path, str]] = []
    for src, dst, reason in actions:
        if not src.exists():
            continue
        try:
            if src.is_dir() and dst.resolve().is_relative_to(src.resolve()):
                continue
        except Exception:
            pass
        if dry_run:
            results.append((src, dst, reason))
            continue
        ensure_dir(dst.parent)
        moved_src, moved_dst = move_path(src, dst, dry_run=False)
        append(cfg, JournalEntry(ts=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                                 op="move",
                                 src=str(moved_src),
                                 dst=str(moved_dst),
                                 reason=reason))
        results.append((moved_src, moved_dst, reason))
    return results

def adopt_home(cfg: dict, dry_run: bool = False) -> List[Tuple[Path, Path, str]]:
    home = Path.home()
    workspace = Path(cfg["workspace_root"]).expanduser()
    ensure_dir(workspace)
    allowed = set(cfg.get("adopt_home_allow", []))
    std = {"Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos", "Public", "Templates"}
    actions: List[Tuple[Path, Path, str]] = []
    for item in home.iterdir():
        if item.name.startswith(".") or item.name in std:
            continue
        if item.name not in allowed:
            continue
        if item.is_dir() or item.is_file():
            dst = workspace / item.name
            actions.append((item, dst, "adopt home folder into Workspace"))
    return apply(actions, cfg, dry_run=dry_run)

def summary(actions: List[Tuple[Path, Path, str]]) -> dict:
    counts = {}
    for _, _, reason in actions:
        counts[reason] = counts.get(reason, 0) + 1
    return {"total": len(actions), "by_reason": counts}
