"""Generate documentation metadata from the package manifest."""
import json
import re
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parent.parent
package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
version_tag = "v" + package["version"]
(root / "docs/version.json").write_text(
    json.dumps({"version": package["version"]}) + "\n", encoding="utf-8"
)
# Bake the manifest version into the page so no runtime version.json fetch is
# needed. The data-package-version hooks stay as documentation of intent.
page = root / "docs/index.html"
html = page.read_text(encoding="utf-8")
stamped = re.sub(
    r'data-package-version>([^<]*)<',
    lambda match: f"data-package-version>{version_tag}<",
    html,
)
if stamped != html:
    page.write_text(stamped, encoding="utf-8")
