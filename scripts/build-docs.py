"""Generate documentation metadata from the package manifest."""
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parent.parent
package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
(root / "docs/version.json").write_text(
    json.dumps({"version": package["version"]}) + "\n", encoding="utf-8"
)
