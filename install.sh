#!/bin/sh
# Installs Bellwether without cloning anything or needing Rust:
#   curl -fsSL https://raw.githubusercontent.com/mashware/bellwether/main/install.sh | sh
# It puts the MCP server in ~/.local/bin and the mods, layout and theme in Herdr's folder, registers
# the server with Claude Code and Codex when they are installed, and switches Herdr to the layout.
# Run it again to update.
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

# The server, from the latest release, checked against the checksum published beside it.
asset=bellwether-panel-mcp-$target.tar.gz
curl -fsSL -o "$tmp/$asset" "https://github.com/$repo/releases/latest/download/$asset"
curl -fsSL -o "$tmp/$asset.sha256" "https://github.com/$repo/releases/latest/download/$asset.sha256"
if command -v sha256sum >/dev/null 2>&1; then (cd "$tmp" && sha256sum -c "$asset.sha256" >/dev/null)
else (cd "$tmp" && shasum -a 256 -c "$asset.sha256" >/dev/null); fi \
  || { echo "bellwether: the download does not match its checksum" >&2; exit 1; }
tar -xzf "$tmp/$asset" -C "$tmp"
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

# Register the server with the agents that are installed, unless it already is.
if command -v claude >/dev/null 2>&1; then
  if claude mcp get bellwether >/dev/null 2>&1; then echo "Claude Code: the bellwether server is already registered."
  else claude mcp add -s user bellwether -- "$bin/bellwether-panel-mcp" >/dev/null && echo "Claude Code: registered the bellwether server."; fi
fi
codex_config="$HOME/.codex/config.toml"
if command -v codex >/dev/null 2>&1 || [ -f "$codex_config" ]; then
  if [ -f "$codex_config" ] && grep -q '^\[mcp_servers\.bellwether\]' "$codex_config"; then echo "Codex: the bellwether server is already registered."
  else
    mkdir -p "$(dirname "$codex_config")"
    # Codex passes only the variables it is told to; the server needs Herdr's HC_TERMINAL_ID.
    printf '\n[mcp_servers.bellwether]\ncommand = "%s"\nenv_vars = ["HC_TERMINAL_ID", "BELLWETHER_STATE"]\n' "$bin/bellwether-panel-mcp" >> "$codex_config"
    echo "Codex: registered the bellwether server."
  fi
fi

# Show it, when Herdr is running.
if command -v herdr-app >/dev/null 2>&1 && herdr-app do layout.switch --name bellwether >/dev/null 2>&1; then
  echo "Herdr: switched to the bellwether layout (ctrl-shift-l switches back)."
else
  echo "Herdr: open it and run  herdr-app do layout.switch --name bellwether"
fi

echo
echo "Bellwether $tag installed. Restart your agent sessions so they load the server."
