"""Regenerate package-manager manifests for a release tag.

Reads the release archives from a directory and rewrites the Homebrew
formula, the Scoop bucket manifest and the WinGet manifests in place.
Every replacement must match exactly once; anything unexpected fails loudly
instead of writing a half-updated tree. Re-running with the same inputs
changes nothing.

Usage: rehash-manifests.py --tag v1.2.3 --assets-dir <dir> [--root <repo>]
The assets directory must contain source.tar.gz and
ncell-cli-<tag>-windows-x86_64.zip.
"""

import argparse
import hashlib
import re
import shutil
import sys
import zipfile
from pathlib import Path

VERSION = re.compile(r"\d+\.\d+\.\d+")


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"rehash-manifests: {message}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def replace_once(path: Path, pattern: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    matches = len(re.findall(pattern, text))
    if matches != 1:
        fail(f"{path}: expected 1 match for {pattern!r}, found {matches}")
    path.write_text(re.sub(pattern, replacement, text, count=1), encoding="utf-8")


def latest_winget(winget: Path, target: str) -> Path:
    candidates = [
        entry
        for entry in winget.iterdir()
        if entry.is_dir() and VERSION.fullmatch(entry.name) and entry.name != target
    ]
    if not candidates:
        fail(f"{winget}: no previous version directory to copy from")
    return sorted(candidates, key=lambda entry: tuple(int(part) for part in entry.name.split(".")))[-1]


def main(tag: str, assets_dir: Path, root: Path) -> None:
    version = tag[1:] if tag.startswith("v") else tag
    if not VERSION.fullmatch(version):
        fail(f"tag {tag!r} is not a plain vX.Y.Z release tag")

    win_zip = assets_dir / f"ncell-cli-{tag}-windows-x86_64.zip"
    tarball = assets_dir / "source.tar.gz"
    for archive in (win_zip, tarball):
        if not archive.is_file():
            fail(f"missing archive: {archive}")
    with zipfile.ZipFile(win_zip) as archive:
        names = set(archive.namelist())
        if "ncell.exe" not in names or "LICENSE" not in names:
            fail(f"{win_zip}: expected ncell.exe and LICENSE, found {sorted(names)}")
    win_hash = sha256_file(win_zip)
    tar_hash = sha256_file(tarball)

    manifest = (root / "Cargo.toml").read_text(encoding="utf-8")
    package_version = re.search(r'\nversion = "([^"]+)"', manifest)
    if package_version is None or package_version.group(1) != version:
        found = package_version.group(1) if package_version else None
        fail(f"Cargo.toml version {found!r} does not match tag {tag!r}")

    formula = root / "Formula/ncell-cli.rb"
    replace_once(
        formula,
        r'url "https://github\.com/rohityadav-sas/ncell-cli/archive/refs/tags/v[^"]+\.tar\.gz"',
        f'url "https://github.com/rohityadav-sas/ncell-cli/archive/refs/tags/{tag}.tar.gz"',
    )
    replace_once(formula, r'\n  version "[^"]+"', f'\n  version "{version}"')
    replace_once(formula, r'\n  sha256 "[0-9a-f]+"', f'\n  sha256 "{tar_hash}"')
    replace_once(
        formula, r"ncell-cli \d+\.\d+\.\d+", f"ncell-cli {version}"
    )

    bucket = root / "bucket/ncell-cli.json"
    replace_once(bucket, r'"version": "[^"]+"', f'"version": "{version}"')
    replace_once(
        bucket,
        r'"url": "https://github\.com/rohityadav-sas/ncell-cli/releases/download/v[^"]+/ncell-cli-v[^-"]+-windows-x86_64\.zip"',
        f'"url": "https://github.com/rohityadav-sas/ncell-cli/releases/download/{tag}/ncell-cli-{tag}-windows-x86_64.zip"',
    )
    replace_once(bucket, r'"hash": "[0-9a-f]+"', f'"hash": "{win_hash}"')

    winget = root / "packaging/winget"
    target = winget / version
    if not target.is_dir():
        shutil.copytree(latest_winget(winget, version), target)
    installer = target / "RohitYadav.NcellCLI.installer.yaml"
    version_file = target / "RohitYadav.NcellCLI.yaml"
    locale = target / "RohitYadav.NcellCLI.locale.en-US.yaml"
    for path in (installer, version_file, locale):
        if not path.is_file():
            fail(f"missing manifest: {path}")
    replace_once(version_file, r"PackageVersion: \d+\.\d+\.\d+", f"PackageVersion: {version}")
    replace_once(locale, r"PackageVersion: \d+\.\d+\.\d+", f"PackageVersion: {version}")
    replace_once(
        locale,
        r"ReleaseNotesUrl: https://github\.com/rohityadav-sas/ncell-cli/releases/tag/v[^\s]+",
        f"ReleaseNotesUrl: https://github.com/rohityadav-sas/ncell-cli/releases/tag/{tag}",
    )
    replace_once(installer, r"PackageVersion: \d+\.\d+\.\d+", f"PackageVersion: {version}")
    replace_once(
        installer,
        r"InstallerUrl: https://github\.com/rohityadav-sas/ncell-cli/releases/download/v[^\s]+",
        f"InstallerUrl: https://github.com/rohityadav-sas/ncell-cli/releases/download/{tag}/ncell-cli-{tag}-windows-x86_64.zip",
    )
    replace_once(
        installer, r"InstallerSha256: [0-9A-Fa-f]+", f"InstallerSha256: {win_hash.upper()}"
    )

    # Read everything back: the tree must now name this tag everywhere.
    check = "\n".join(
        [
            formula.read_text(encoding="utf-8"),
            bucket.read_text(encoding="utf-8"),
            installer.read_text(encoding="utf-8"),
            version_file.read_text(encoding="utf-8"),
            locale.read_text(encoding="utf-8"),
        ]
    )
    for token in (tag, version, tar_hash, win_hash, win_hash.upper()):
        if token not in check:
            fail(f"verification failed: {token!r} missing after rewrite")
    print(f"manifests point at {tag} (tarball {tar_hash[:12]}…, zip {win_hash[:12]}…)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--assets-dir", required=True)
    parser.add_argument("--root", default=str(Path(__file__).resolve().parent.parent))
    args = parser.parse_args()
    try:
        main(args.tag, Path(args.assets_dir), Path(args.root))
    except SystemExit as error:
        if error.code:
            print(error, file=sys.stderr)
        raise
