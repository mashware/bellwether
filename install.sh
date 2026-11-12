#!/bin/sh
# Installs Bellwether without cloning anything or needing Rust:
#   curl -fsSL https://raw.githubusercontent.com/mashware/bellwether/main/install.sh | sh
# It puts the MCP server in ~/.local/bin, the mods, layout and theme in Herdr's folder, and tells
# you how to register the server with your agents. Run it again to update.
set -eu

repo=mashware/bellwether
bin=${BELLWETHER_BIN:-"$HOME/.local/bin"}
app=${HERDR_APP_HOME:-"$HOME/.config/herdr-app"}

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "bellwether: no prebuilt server for $(uname -s) $(uname -m); build it from source (see the README)" >&2; exit 1 ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The server, from the latest release.
curl -fsSL "https://github.com/$repo/releases/latest/download/bellwether-panel-mcp-$target.tar.gz" | tar -xz -C "$tmp"
mkdir -p "$bin"
# Copied beside and renamed over, so a running server is not hit with "text file busy".
cp "$tmp/bellwether-panel-mcp" "$bin/.bellwether-panel-mcp.new"
chmod 755 "$bin/.bellwether-panel-mcp.new"
mv "$bin/.bellwether-panel-mcp.new" "$bin/bellwether-panel-mcp"

# The mods, layout and theme, from the same release's source.
tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest" | sed 's#.*/tag/##')
curl -fsSL "https://github.com/$repo/archive/refs/tags/$tag.tar.gz" | tar -xz -C "$tmp"
src=$(find "$tmp" -maxdepth 1 -type d -name 'bellwether-*' | head -n 1)
mkdir -p "$app/mods" "$app/layouts" "$app/themes"
for mod in "$src"/mods/*/; do
  name=$(basename "$mod")
  mkdir -p "$app/mods/$name"
  cp -R "$mod". "$app/mods/$name/"
done
cp "$src"/layouts/*.json "$app/layouts/"
cp "$src"/themes/*.json "$app/themes/"

echo "Bellwether $tag installed."
echo
echo "Switch Herdr to it:   herdr-app do layout.switch --name bellwether"
echo "Register the server:"
echo "  Claude Code:  claude mcp add -s user bellwether -- $bin/bellwether-panel-mcp"
echo "  Codex, in ~/.codex/config.toml:"
echo "    [mcp_servers.bellwether]"
echo "    command = \"$bin/bellwether-panel-mcp\""
echo "    env_vars = [\"HC_TERMINAL_ID\", \"BELLWETHER_STATE\"]"
echo "Then restart your agent sessions."
