#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

# Ignore executable-bit-only changes in this repository so chmod never blocks pull.
git config core.fileMode false

# Refuse to overwrite real local content changes. Mode-only changes are ignored above.
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Local content changes detected; not pulling automatically." >&2
  echo "Review with: git status --short && git diff" >&2
  exit 2
fi

# Keep untracked build/log artifacts from blocking normal sync; they are left untouched.
git fetch origin
git switch paw3222-v7-runtime-ui
git pull --ff-only

# No chmod required anywhere: invoke the build script explicitly with bash.
bash build-v7.sh
