from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

PROJECT_MARKERS = {
    ".git", "Cargo.toml", "pyproject.toml", "package.json", "go.mod", "pom.xml",
    "requirements.txt", "uv.lock", "Makefile", "CMakeLists.txt", "Dockerfile", "README.md"
}

IMAGE_EXT = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".svg", ".heic", ".avif"}
DOC_EXT = {".pdf", ".doc", ".docx", ".odt", ".txt", ".rtf", ".md", ".html", ".htm", ".csv", ".json", ".jsonl", ".yaml", ".yml", ".tsv"}
VIDEO_EXT = {".mp4", ".mov", ".mkv", ".avi", ".webm", ".m4v"}
AUDIO_EXT = {".mp3", ".wav", ".flac", ".ogg", ".m4a", ".aac"}
ARCHIVE_EXT = {".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".zst"}
CODE_EXT = {".py", ".rs", ".js", ".ts", ".tsx", ".jsx", ".go", ".java", ".c", ".cpp", ".h", ".hpp", ".sh", ".bash", ".toml", ".make", ".cmake"}
MODEL_EXT = {".gguf", ".safetensors", ".onnx", ".pt", ".pth", ".bin", ".ggml"}
DATASET_EXT = {".parquet", ".arrow", ".feather", ".sqlite", ".db", ".duckdb", ".h5", ".hdf5", ".npz"}
ISO_EXT = {".iso", ".img", ".qcow2", ".vhdx", ".vmdk"}
SCREENSHOT_HINTS = ("screenshot", "screen shot", "scrot", "snipping", "shot_")

@dataclass(frozen=True)
class Decision:
    category: str
    destination: Optional[Path]
    reason: str

def is_project_dir(path: Path) -> bool:
    if not path.is_dir():
        return False
    names = {p.name for p in path.iterdir() if p.exists()}
    return any(marker in names for marker in PROJECT_MARKERS)

def classify_file(path: Path, cfg: dict) -> Decision:
    name_l = path.name.lower()
    ext = path.suffix.lower()
    workspace_root = Path(cfg["workspace_root"]).expanduser()

    if ext in ISO_EXT:
        return Decision("isos", workspace_root / "Temp" / "ISOs", f"disk image ({ext})")
    if ext in MODEL_EXT:
        return Decision("models", Path(cfg["models_root"]).expanduser(), f"model artifact ({ext})")
    if ext in DATASET_EXT:
        return Decision("datasets", Path(cfg["datasets_root"]).expanduser(), f"dataset-like data ({ext})")
    if any(h in name_l for h in SCREENSHOT_HINTS) and ext in IMAGE_EXT:
        return Decision("screenshots", Path(cfg["screenshots_root"]).expanduser(), "screenshot filename")
    if ext in IMAGE_EXT:
        return Decision("images", workspace_root / "Assets" / "Images", f"image ({ext})")
    if ext in VIDEO_EXT:
        return Decision("videos", workspace_root / "Assets" / "Video", f"video ({ext})")
    if ext in AUDIO_EXT:
        return Decision("audio", workspace_root / "Assets" / "Audio", f"audio ({ext})")
    if ext in ARCHIVE_EXT:
        return Decision("archives", workspace_root / "Temp" / "Archives", f"archive ({ext})")
    if ext in CODE_EXT:
        return Decision("code", workspace_root / "Projects" / "Playground", f"code file ({ext})")
    if ext in DOC_EXT:
        return Decision("documents", workspace_root / "Notes", f"document ({ext})")
    return Decision("misc", Path(cfg["quarantine_dir"]).expanduser(), "unclassified")

def classify_dir(path: Path, cfg: dict) -> Decision:
    if is_project_dir(path):
        return Decision("project", Path(cfg["project_root"]).expanduser() / path.name, "project markers found")
    return Decision("archive", Path(cfg["archive_root"]).expanduser() / path.name, "directory without project markers")
