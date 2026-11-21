#!/bin/sh
# Re-download the Tabler icons Forkline uses (MIT, https://tabler.io/icons).
# Each file is saved under the Lucide name gpui-kit's `IconName` asks for, so
# `crate::assets` serves Tabler in place of the kit's Lucide icon.
# Add a line here (and to `TABLER` in src/assets.rs) before using a new icon.
set -e
cd "$(dirname "$0")"
V=3.52.0
CDN="https://cdn.jsdelivr.net/npm/@tabler/icons@$V/icons"

# lucide-name tabler-name (prefix "filled/" for a filled icon)
while read -r lucide tabler; do
  case "$tabler" in
    filled/*) url="$CDN/filled/${tabler#filled/}.svg" ;;
    *) url="$CDN/outline/$tabler.svg" ;;
  esac
  curl -sfL "$url" -o "$lucide.svg" || { echo "missing: $tabler"; exit 1; }
done <<'EOF'
arrow-down arrow-down
arrow-left arrow-left
arrow-right arrow-right
arrow-up arrow-up
bell bell
book-marked book
book-open book
calendar calendar
check check
chevron-down chevron-down
chevron-left chevron-left
chevron-right chevron-right
chevron-up chevron-up
chevrons-up-down selector
circle-alert alert-circle
circle-check circle-check
circle-dot circle-dot
circle-user user-circle
circle-x circle-x
clock clock
close x
columns-2 columns-2
compass compass
copy copy
dash minus
ellipsis dots
ellipsis-vertical dots-vertical
external-link external-link
eye eye
eye-off eye-off
file file
file-text file-text
folder folder
folder-closed folder
folder-open folder-open
git-commit-horizontal git-commit
git-fork git-fork
git-merge git-merge
git-pull-request git-pull-request
git-pull-request-closed git-pull-request-closed
git-pull-request-draft git-pull-request-draft
globe world
house home
inbox inbox
info info-circle
loader loader
loader-circle loader-2
menu menu-2
minus minus
moon moon
palette palette
panel-left layout-sidebar
panel-left-close layout-sidebar-left-collapse
panel-left-open layout-sidebar-left-expand
pin pin
plus plus
refresh-cw refresh
search search
settings settings
settings-2 adjustments-horizontal
star star
star-fill filled/star
star-off star-off
sun sun
triangle-alert alert-triangle
user user
x x
EOF
echo "ok: $(ls ./*.svg | wc -l) icons"
