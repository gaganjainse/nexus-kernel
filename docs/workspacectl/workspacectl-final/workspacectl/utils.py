from __future__ import annotations
import shutil
from pathlib import Path

def ensure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)

def unique_destination(dst: Path) -> Path:
    if not dst.exists():
        return dst
    stem = dst.stem
    suffix = dst.suffix
    parent = dst.parent
    i = 1
    while True:
        candidate = parent / f"{stem} ({i}){suffix}"
        if not candidate.exists():
            return candidate
        i += 1

def move_path(src: Path, dst: Path, dry_run: bool = False) -> tuple[Path, Path]:
    dst = unique_destination(dst)
    if dry_run:
        return src, dst
    ensure_dir(dst.parent)
    shutil.move(str(src), str(dst))
    return src, dst
