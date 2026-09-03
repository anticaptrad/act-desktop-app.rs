#!/usr/bin/env bash
set -euo pipefail

generator_commit="a79daa64fcfeb31d8052d92273e8b78472c78b20"

if ! command -v f2e >/dev/null 2>&1; then
  printf 'f2e is required; install flags-2-env-cli at %s\n' "$generator_commit" >&2
  exit 2
fi

python3 scripts/generated_guard.py unfreeze --root . || true
rm -rf \
  generated/dart \
  generated/gleam \
  generated/typescript
rm -f generated/README.md

f2e generate .cli-flags.toml --out generated --name DesktopEnv --lang rust

# f2e writes a generic uppercase README. Replace it with this repository's
# reviewed lowercase notice and machine-readable provenance policy.
python3 scripts/generated_guard.py unfreeze --root .
rm -f generated/README.md
cp policy/generated-readme.md generated/readme.md
cp policy/generated-manifest.json generated/manifest.json

python3 scripts/generated_guard.py check --root . --require-manifest
python3 scripts/generated_guard.py freeze --root .
python3 scripts/generated_guard.py check --root . --require-manifest --require-frozen
