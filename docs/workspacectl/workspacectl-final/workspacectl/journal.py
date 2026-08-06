from __future__ import annotations
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import List

@dataclass
class JournalEntry:
    ts: str
    op: str
    src: str
    dst: str
    reason: str

def now() -> str:
    return datetime.now(timezone.utc).isoformat()

def journal_path(cfg: dict) -> Path:
    base = Path(cfg["journal_dir"]).expanduser()
    base.mkdir(parents=True, exist_ok=True)
    return base / "journal.jsonl"

def append(cfg: dict, entry: JournalEntry) -> None:
    p = journal_path(cfg)
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open("a", encoding="utf-8") as f:
        f.write(json.dumps(entry.__dict__, ensure_ascii=False) + "\n")

def read_all(cfg: dict) -> List[JournalEntry]:
    p = journal_path(cfg)
    if not p.exists():
        return []
    entries: List[JournalEntry] = []
    for line in p.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        d = json.loads(line)
        entries.append(JournalEntry(**d))
    return entries

def clear(cfg: dict) -> None:
    p = journal_path(cfg)
    if p.exists():
        p.unlink()
