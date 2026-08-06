from __future__ import annotations
import json
import os
from pathlib import Path
from typing import Any, Dict

APP_NAME = "workspacectl"

DEFAULT_CONFIG: Dict[str, Any] = {
    "workspace_root": "~/Workspace",
    "watch_paths": ["~/Downloads", "~/Desktop", "~/Pictures", "~/Videos", "~/Music"],
    "dry_run": True,
    "journal_dir": "~/.local/share/workspacectl",
    "quarantine_dir": "~/Workspace/Temp/Quarantine",
    "project_root": "~/Workspace/Projects/Active",
    "archive_root": "~/Workspace/Projects/Archive",
    "models_root": "~/Workspace/AI/Models",
    "datasets_root": "~/Workspace/AI/Datasets",
    "screenshots_root": "~/Pictures/Screenshots",
    "skip_dirs": [
        ".git", ".cache", ".config", ".local", ".cargo", ".rustup", ".npm", ".venv", "venv",
        "__pycache__", "node_modules", "target", ".idea", ".vscode"
    ],
    "adopt_home_allow": [
        "AI", "Datasets", "Models", "Projects", "Scripts", "Workspace", "Notes", "Containers"
    ],
}

def expand(path: str | Path) -> Path:
    return Path(os.path.expandvars(os.path.expanduser(str(path)))).resolve()

def config_dir() -> Path:
    return expand("~/.config/workspacectl")

def config_path() -> Path:
    return config_dir() / "config.json"

def journal_dir(cfg: Dict[str, Any]) -> Path:
    return expand(cfg["journal_dir"])

def load_config() -> Dict[str, Any]:
    cfg = dict(DEFAULT_CONFIG)
    path = config_path()
    if path.exists():
        try:
            cfg.update(json.loads(path.read_text(encoding="utf-8")))
        except Exception:
            pass
    return cfg

def save_config(cfg: Dict[str, Any]) -> Path:
    config_dir().mkdir(parents=True, exist_ok=True)
    path = config_path()
    path.write_text(json.dumps(cfg, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path
