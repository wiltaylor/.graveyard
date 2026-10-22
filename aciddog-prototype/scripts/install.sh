#!/usr/bin/env bash
#
# Install aciddog and aciddog-manager from a GitHub release.
#
# Usage: install.sh [--version <tag>] [--bin-dir <dir>]
#
#   --version  Tag to install (e.g. v0.1.0-alpha). Default: latest pre-release.
#   --bin-dir  Where to install binaries. Default: $HOME/.local/bin.
#
# The aciddog-manager binary embeds its web frontend, so no separate web
# assets or wrapper script are required. The WCL schema is installed under
# ${XDG_DATA_HOME:-$HOME/.local/share}/wcl/lib/ for `import <aciddog.wcl>`.

set -euo pipefail

REPO="${REPO:-wiltaylor/aciddog}"
VERSION=""
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
WCL_LIB_DIR="$DATA_HOME/wcl/lib"

die() { echo "error: $*" >&2; exit 1; }
note() { echo "==> $*"; }

usage() {
  sed -n '3,12p' "$0" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)  VERSION="${2:?--version requires a tag}"; shift 2 ;;
    --bin-dir)  BIN_DIR="${2:?--bin-dir requires a path}"; shift 2 ;;
    -h|--help)  usage; exit 0 ;;
    *)          die "unknown argument: $1 (try --help)" ;;
  esac
done

OS="$(uname -s)"
ARCH="$(uname -m)"
if [[ "$OS" != "Linux" || "$ARCH" != "x86_64" ]]; then
  die "unsupported platform: $OS $ARCH (only Linux x86_64 is published right now)"
fi
PLATFORM="linux-x64"

for cmd in curl tar sha256sum sed grep mktemp; do
  command -v "$cmd" >/dev/null 2>&1 || die "missing required command: $cmd"
done

if [[ -z "$VERSION" ]]; then
  note "Resolving latest pre-release from github.com/$REPO"
  VERSION="$(
    curl -fsSL "https://api.github.com/repos/${REPO}/releases" |
    grep -E '"(tag_name|prerelease)"' |
    # Pair each tag_name with the prerelease bool that follows it, then
    # print the first tag_name whose prerelease == true.
    awk '
      /"tag_name"/ { match($0, /"tag_name": "[^"]+"/); tag = substr($0, RSTART+13, RLENGTH-14); next }
      /"prerelease"/ {
        if ($0 ~ /true/ && tag != "") { print tag; exit }
        tag = ""
      }
    '
  )"
  [[ -n "$VERSION" ]] || die "no pre-release found in github.com/$REPO/releases"
  note "Latest pre-release: $VERSION"
fi

if [[ "$VERSION" != v* ]]; then
  VERSION="v${VERSION}"
fi

VER_NO_V="${VERSION#v}"
TARBALL="aciddog-${VER_NO_V}-${PLATFORM}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"

TMPDIR="$(mktemp -d -t aciddog-install.XXXXXX)"
trap 'rm -rf "$TMPDIR"' EXIT

note "Downloading $TARBALL"
curl -fsSL -o "$TMPDIR/$TARBALL" "$BASE_URL/$TARBALL"

note "Downloading SHA256SUMS"
curl -fsSL -o "$TMPDIR/SHA256SUMS" "$BASE_URL/SHA256SUMS"

note "Verifying checksum"
( cd "$TMPDIR" && sha256sum -c --ignore-missing SHA256SUMS )

note "Extracting"
tar -xzf "$TMPDIR/$TARBALL" -C "$TMPDIR"
STAGE_DIR="$TMPDIR/aciddog-${VER_NO_V}-${PLATFORM}"
[[ -d "$STAGE_DIR" ]] || die "expected $STAGE_DIR after extraction but it's missing"

mkdir -p "$BIN_DIR"

note "Installing aciddog → $BIN_DIR/aciddog"
install -m 0755 "$STAGE_DIR/bin/aciddog" "$BIN_DIR/aciddog"

note "Installing aciddog-manager → $BIN_DIR/aciddog-manager"
install -m 0755 "$STAGE_DIR/bin/aciddog-manager" "$BIN_DIR/aciddog-manager"

mkdir -p "$WCL_LIB_DIR"
install -m 0644 "$STAGE_DIR/share/wcl/aciddog.wcl" "$WCL_LIB_DIR/aciddog.wcl"
note "Installed WCL schema → $WCL_LIB_DIR/aciddog.wcl"

echo
note "Installed aciddog $VERSION"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "    note: $BIN_DIR is not on PATH; add it to use the binaries directly" ;;
esac
