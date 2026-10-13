"""Generate documentation metadata from the package manifest."""
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parent.parent
package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
(root / "docs/version.json").write_text(
    json.dumps({"version": package["version"]}) + "\n", encoding="utf-8"
)
# Stamp the manifest version into the banner badges so the artwork never
# shows a stale release. SVG cannot fetch version.json at runtime.
import re

version_tag = "v" + package["version"]
version_pattern = re.compile(r"v\d+\.\d+\.\d+(?:-[\w.-]+)?(?:\+[\w.-]+)?")
for asset in ("docs/assets/banner.svg", "docs/assets/banner-mobile.svg"):
    path = root / asset
    text = path.read_text(encoding="utf-8")
    updated = version_pattern.sub(version_tag, text)
    if updated != text:
        path.write_text(updated, encoding="utf-8")
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
