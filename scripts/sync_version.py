"""Synchronize package version from Cargo.toml across all documentation, installer scripts, and manifests."""

import json
import re
import subprocess
from pathlib import Path
import tomllib


def sync():
    root = Path(__file__).resolve().parent.parent
    cargo_path = root / "Cargo.toml"
    package = tomllib.loads(cargo_path.read_text(encoding="utf-8"))["package"]
    version = package["version"]
    version_tag = "v" + version

    # 1. Update docs/version.json
    version_json = root / "docs/version.json"
    new_version_json_content = json.dumps({"version": version}) + "\n"
    if not version_json.exists() or version_json.read_text(encoding="utf-8") != new_version_json_content:
        version_json.write_text(new_version_json_content, encoding="utf-8")

    # 2. Update docs/index.html
    docs_page = root / "docs/index.html"
    if docs_page.exists():
        html = docs_page.read_text(encoding="utf-8")
        stamped = re.sub(
            r"data-package-version>([^<]*)<",
            f"data-package-version>{version_tag}<",
            html,
        )
        stamped = re.sub(
            r"NCELL_VERSION=v[0-9]+\.[0-9]+\.[0-9]+",
            f"NCELL_VERSION={version_tag}",
            stamped,
        )
        stamped = re.sub(
            r"(\$env:NCELL_VERSION\s*=\s*['\"])v[0-9]+\.[0-9]+\.[0-9]+(['\"])",
            rf"\g<1>{version_tag}\g<2>",
            stamped,
        )
        if stamped != html:
            docs_page.write_text(stamped, encoding="utf-8")

    # 3. Update README.md
    readme = root / "README.md"
    if readme.exists():
        content = readme.read_text(encoding="utf-8")
        updated = re.sub(
            r"NCELL_VERSION=v[0-9]+\.[0-9]+\.[0-9]+",
            f"NCELL_VERSION={version_tag}",
            content,
        )
        updated = re.sub(
            r"(\$env:NCELL_VERSION\s*=\s*['\"])v[0-9]+\.[0-9]+\.[0-9]+(['\"])",
            rf"\g<1>{version_tag}\g<2>",
            updated,
        )
        if updated != content:
            readme.write_text(updated, encoding="utf-8")

    # 4. Update install.sh
    install_sh = root / "install.sh"
    if install_sh.exists():
        content = install_sh.read_text(encoding="utf-8")
        updated = re.sub(
            r"(#\s*NCELL_VERSION\s+release tag to install \(default: latest\), e\.g\.\s*)v[0-9]+\.[0-9]+\.[0-9]+",
            rf"\g<1>{version_tag}",
            content,
        )
        if updated != content:
            install_sh.write_text(updated, encoding="utf-8")

    # 5. Update install.ps1
    install_ps1 = root / "install.ps1"
    if install_ps1.exists():
        content = install_ps1.read_text(encoding="utf-8")
        updated = re.sub(
            r"(#\s*NCELL_VERSION\s+release tag to install \(default: latest\), e\.g\.\s*)v[0-9]+\.[0-9]+\.[0-9]+",
            rf"\g<1>{version_tag}",
            content,
        )
        if updated != content:
            install_ps1.write_text(updated, encoding="utf-8")

    # 6. Update Cargo.lock if cargo is available
    cargo_lock = root / "Cargo.lock"
    if cargo_lock.exists():
        try:
            subprocess.run(
                ["cargo", "check", "--offline"],
                cwd=root,
                capture_output=True,
                check=False,
            )
        except Exception:
            pass


if __name__ == "__main__":
    sync()
