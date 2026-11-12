#!/bin/sh
# Copies this project's mods and layout into the Herdr app's folder and checks each mod.
# Herdr's hot reload does not follow a symlinked mod folder, so the files are copied.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app=${HERDR_APP_HOME:-"$HOME/.config/herdr-app"}

for mod in "$here"/mods/*/; do
  name=$(basename "$mod")
  mkdir -p "$app/mods/$name"
  cp -R "$mod". "$app/mods/$name/"
done
cp "$here"/layouts/*.json "$app/layouts/"
mkdir -p "$app/themes"
cp "$here"/themes/*.json "$app/themes/"

status=0
for mod in "$here"/mods/*/; do
  herdr-app check "$app/mods/$(basename "$mod")" || status=1
done
exit $status
