#!/usr/bin/env bash
set -euo pipefail

generator_commit="a79daa64fcfeb31d8052d92273e8b78472c78b20"

if ! command -v f2e >/dev/null 2>&1; then
  printf 'f2e is required; install flags-2-env-cli at %s\n' "$generator_commit" >&2
  exit 2
fi

# This native Rust desktop repository intentionally keeps only its Rust
# projection. Stale cross-language output must be removed through reviewed Git
# changes, not by an unscoped deletion hidden inside the generator.
for stale in generated/dart generated/gleam generated/typescript; do
  if [[ -e "$stale" ]]; then
    printf 'stale generated language tree must be removed through Git first: %s\n' "$stale" >&2
    exit 1
  fi
done

python3 scripts/generated_guard.py unfreeze --root . || true

scratch_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/act-config-generation-${$}"
mkdir -p "$scratch_root"
f2e generate .cli-flags.toml --out "$scratch_root" --name DesktopEnv --lang rust

mkdir -p generated/rust generated/json-schema
install -m 0644 "$scratch_root/rust/env.rs" generated/rust/env.rs
install -m 0644 "$scratch_root/rust/runtime.rs" generated/rust/runtime.rs
install -m 0644 "$scratch_root/json-schema/env.os.schema.json" generated/json-schema/env.os.schema.json
install -m 0644 "$scratch_root/json-schema/env.values.schema.json" generated/json-schema/env.values.schema.json

# Keep the repository-reviewed lowercase notice and machine-readable provenance
# policy instead of the generator's generic uppercase README.
install -m 0644 policy/generated-readme.md generated/readme.md
install -m 0644 policy/generated-manifest.json generated/manifest.json

python3 scripts/generated_guard.py check --root . --require-manifest
python3 scripts/generated_guard.py freeze --root .
python3 scripts/generated_guard.py check --root . --require-manifest --require-frozen
