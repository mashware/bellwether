#!/bin/sh
# Loads and renders every mod of this repository once with `herdr-app check`, straight from this
# folder: unlike scripts/sync.sh, it touches nothing in the Herdr app's own folder.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
status=0
for mod in "$here"/mods/*/; do
  herdr-app check "$mod" || status=1
done
exit $status
