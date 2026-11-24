#!/bin/sh
# Prints CHANGELOG.md's section for version tag $1 ("## v0.7.0" up to the
# next "## "), and fails when it is missing or empty: every release says
# what changed, for its users.
set -eu
tag="$1"
notes=$(awk -v head="## $tag" '$0 == head { on = 1; next } /^## / { on = 0 } on' CHANGELOG.md)
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
  echo "CHANGELOG.md has no \"## $tag\" section: write what changed, for users, before tagging." >&2
  exit 1
fi
printf '%s\n' "$notes"
