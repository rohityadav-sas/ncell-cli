"""Generate documentation metadata from the package manifest."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from sync_version import sync

if __name__ == "__main__":
    sync()
