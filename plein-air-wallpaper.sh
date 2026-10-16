#!/usr/bin/env bash
# Fetch a painting matching the local weather, recolor it to a palette, and set it as the Noctalia wallpaper.
# Extra args go to plein-air (e.g. --city Paris, --coords 48.85,2.35, --width 2560).
# PLEIN_AIR_PALETTE=none skips recoloring; PLEIN_AIR_LUTGEN_ARGS overrides the lutgen algorithm flags.
set -euo pipefail

dir="${PLEIN_AIR_DIR:-$HOME/Downloads/walls/plein-air}"
bin="${PLEIN_AIR_BIN:-plein-air}"
palette="${PLEIN_AIR_PALETTE:-catppuccin-macchiato}"
read -ra lut_args <<<"${PLEIN_AIR_LUTGEN_ARGS:--R -s 96 -n 8 -P -L 0.5}"
mkdir -p "$dir"

meta=$("$bin" --output "$dir/original" --set-ext "$@")
orig=$(jq -r .path <<<"$meta")
img="$dir/wallpaper.${orig##*.}"
find "$dir" -maxdepth 1 \( -name 'original.*' ! -path "$orig" -o -name 'wallpaper.*' ! -path "$img" \) -delete

if [[ "$palette" != none ]] && ! command -v lutgen >/dev/null; then
  echo "lutgen not found, skipping recolor" >&2
  palette=none
fi

if [[ "$palette" == none ]]; then
  cp "$orig" "$img"
else
  lutgen apply -c -p "$palette" "${lut_args[@]}" "$orig" -o "$img" >/dev/null 2>&1
fi

# Noctalia ignores wallpaper-set for an unchanged path and caches images by path,
# so point it at whichever of two symlinks it isn't currently using.
link="$dir/.current-a"
[[ "$(noctalia msg wallpaper-get)" == "$link" ]] && link="$dir/.current-b"
ln -sfn "$(basename "$img")" "$link"
noctalia msg wallpaper-set "$link"

printf '%s\n' "$meta" > "$dir/current.json"
jq -r '"\(.artist) — \(.title) [\(.source)]"' <<<"$meta"
