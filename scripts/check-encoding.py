"""Reject invalid UTF-8 in tracked source, configuration and documentation."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
suffixes = {".md", ".rs", ".toml", ".lock", ".json", ".yml", ".yaml",
            ".html", ".css", ".js", ".py", ".txt", ".ps1", ".svg"}
names = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode("utf-8").split("\0")
for name in filter(None, names):
    path = root / name
    if path.suffix in suffixes or path.name in {".gitignore", ".gitattributes"}:
        try:
            path.read_bytes().decode("utf-8")
        except UnicodeDecodeError as error:
            raise SystemExit(f"{name}: invalid UTF-8: {error}") from error
print("Tracked text files are valid UTF-8.")
