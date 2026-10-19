#!/bin/sh
# ncell-cli installer for Linux and macOS
#   curl -fsSL https://raw.githubusercontent.com/rohityadav-sas/ncell-cli/main/install.sh | sh
#
# Optional environment variables:
#   NCELL_VERSION      release tag to install (default: latest), e.g. v0.5.3
#   NCELL_INSTALL_DIR  install directory (default: $HOME/.local/bin)
set -eu

REPO="rohityadav-sas/ncell-cli"
BIN="ncell"
INSTALL_DIR="${NCELL_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${NCELL_VERSION:-latest}"

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

case "$INSTALL_DIR" in
  /*) ;;
  *) die "NCELL_INSTALL_DIR must be an absolute path (got '$INSTALL_DIR')" ;;
esac

# --- platform detection -----------------------------------------------------
case "$(uname -s)" in
  Linux)  os="linux" ;;
  Darwin) os="macos" ;;
  *) die "unsupported OS: $(uname -s). On Windows use: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex" ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  arch="x86_64" ;;
  aarch64|arm64) arch="arm64" ;;
  *) die "unsupported architecture: $(uname -m)" ;;
esac
suffix="${os}-${arch}.zip"

# --- downloader -------------------------------------------------------------
if have curl; then
  fetch() { curl -fsSL "$1"; }
  fetch_to() { curl -fsSL "$1" -o "$2"; }
elif have wget; then
  fetch() { wget -qO- "$1"; }
  fetch_to() { wget -qO "$2" "$1"; }
else
  die "curl or wget is required"
fi

# --- find the release asset -------------------------------------------------
if [ "$VERSION" = "latest" ]; then
  api="https://api.github.com/repos/$REPO/releases/latest"
else
  api="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
fi

say "Looking up $VERSION release..."
release="$(fetch "$api")" || die "could not reach GitHub (release '$VERSION' may not exist)"
url="$(printf '%s' "$release" \
  | grep -o "\"browser_download_url\": *\"[^\"]*${suffix}\"" \
  | head -n 1 | cut -d '"' -f 4)"
[ -n "$url" ] || die "no release asset ending in '$suffix' was found"

# --- download and extract ---------------------------------------------------
tmp="$(mktemp -d 2>/dev/null || mktemp -d -t ncell)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading ${url##*/}..."
fetch_to "$url" "$tmp/ncell.zip" || die "download failed"

if have unzip; then
  unzip -q -o "$tmp/ncell.zip" -d "$tmp/out" || die "could not extract the archive"
elif have bsdtar; then
  mkdir -p "$tmp/out" && bsdtar -xf "$tmp/ncell.zip" -C "$tmp/out" || die "could not extract the archive"
else
  die "unzip is required (install it with your package manager, e.g. 'sudo apt install unzip')"
fi

# The release zip keeps the binary at its root next to LICENSE and docs/.
found="$tmp/out/$BIN"
[ -f "$found" ] || die "'$BIN' was not found inside the archive"

# --- install ----------------------------------------------------------------
mkdir -p "$INSTALL_DIR"
chmod +x "$found"
mv -f "$found" "$INSTALL_DIR/$BIN"

say "Installed $BIN to $INSTALL_DIR/$BIN"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) say "Run: $BIN --help" ;;
  *)
    case "$(basename "${SHELL:-}")" in
      zsh)  rc="~/.zshrc" ;;
      bash) if [ "$os" = "macos" ]; then rc="~/.bash_profile"; else rc="~/.bashrc"; fi ;;
      *)    rc="" ;;
    esac
    say ""
    if [ -n "$rc" ]; then
      say "$INSTALL_DIR is not on your PATH. Add this line to $rc, then restart your shell:"
      say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    else
      say "$INSTALL_DIR is not on your PATH. Add it to your PATH, then restart your shell."
    fi
    ;;
esac
