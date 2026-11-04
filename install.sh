#!/bin/sh
# Download a published Trammel release into ~/.local/bin. No root.
# This does not publish anything. A missing release exits with the build command.
set -eu
version="${TRAMMEL_VERSION:-4.0.1}"
os=$(uname -s)
arch=$(uname -m)
case "$os-$arch" in
  Linux-x86_64) asset=trammel-linux-x64 ;;
  Linux-aarch64|Linux-arm64) asset=trammel-linux-arm64 ;;
  Darwin-x86_64) asset=trammel-darwin-x64 ;;
  Darwin-arm64) asset=trammel-darwin-arm64 ;;
  *) echo "No published binary for $os $arch" >&2; exit 1 ;;
esac
url="https://github.com/IronAdamant/Trammel/releases/download/v${version}/${asset}.tar.gz"
sums="https://github.com/IronAdamant/Trammel/releases/download/v${version}/SHA256SUMS"
dest="${HOME}/.local/bin"
mkdir -p "$dest"
tmp=$(mktemp -d)
if ! curl -fsSL "$url" -o "$tmp/${asset}.tar.gz" || ! curl -fsSL "$sums" -o "$tmp/SHA256SUMS"; then
  echo "No GitHub Release v${version} yet. Build from this checkout:" >&2
  echo "  cargo build --release && cp target/release/trammel target/release/trammel-mcp \"$dest/\"" >&2
  exit 1
fi
(cd "$tmp" && grep " ${asset}.tar.gz\$" SHA256SUMS | sha256sum -c -)
tar -xzf "$tmp/${asset}.tar.gz" -C "$dest" --strip-components=1
chmod +x "$dest/trammel" "$dest/trammel-mcp"
echo "installed $dest/trammel"
