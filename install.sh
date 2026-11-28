#!/bin/sh
# Installs `sai` and `saispace-mcp` from the latest GitHub release (Linux and
# macOS). Checks the archive against SHA256SUMS before installing.
#   curl -fsSL https://github.com/saimaneim/saispace/releases/latest/download/install.sh | sh
# Options: SAISPACE_VERSION=v0.1.0  SAISPACE_BIN_DIR=$HOME/.local/bin
set -eu

repo="saimaneim/saispace"
bin_dir="${SAISPACE_BIN_DIR:-$HOME/.local/bin}"
version="${SAISPACE_VERSION:-}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=linux-x64 ;;
  Darwin-arm64) target=macos-arm64 ;;
  Darwin-x86_64) target=macos-x64 ;;
  *) echo "Sistema no soportado: $(uname -s) $(uname -m). Compila con cargo install --path crates/cli" >&2; exit 1 ;;
esac

if [ -z "$version" ]; then
  version=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
fi
[ -n "$version" ] || { echo "No encuentro la última versión" >&2; exit 1; }

archive="saispace-$version-$target.tar.gz"
base="https://github.com/$repo/releases/download/$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/$archive" "$base/$archive"
curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"
expected=$(grep " $archive\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$tmp/$archive" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$tmp/$archive" | cut -d' ' -f1)
fi
[ -n "$expected" ] && [ "$expected" = "$actual" ] || { echo "El checksum no coincide" >&2; exit 1; }

mkdir -p "$bin_dir"
tar -xzf "$tmp/$archive" -C "$tmp"
install -m 755 "$tmp/sai" "$tmp/saispace-mcp" "$bin_dir/"
echo "Instalado $version en $bin_dir. Siguiente: sai setup <agente> y sai doctor"
case ":$PATH:" in *":$bin_dir:"*) ;; *) echo "Añade $bin_dir a tu PATH" ;; esac
