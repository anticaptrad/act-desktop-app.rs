# `generated/` — ACT desktop configuration projections

**Do not edit files in this directory directly.** Everything here except this
notice and `manifest.json` is derivative output. Change the documented
human-authored source, run the pinned generator, and review the complete diff.

## What belongs here

`act-desktop-app.rs` is a native Rust + Qt Quick/QML application. The retained
configuration projection is Rust-only because this repository's executable
imports `generated/rust/env.rs` and `generated/rust/runtime.rs`. Dart, Gleam,
and TypeScript bindings belong in their actual Flutter, client, interface, or
package repositories rather than this native desktop repository.

This desktop application does not expose a general-purpose inbound API merely
because it consumes generated types. Its UI boundary is Qt Quick/QML ↔ CXX-Qt ↔
Rust. The inspected networking exception is a loopback UDP transport diagnostic.
Any future local listener must be explicitly documented, purpose-limited,
loopback-only by default, and authenticated when it crosses a trust boundary.

## Authority classification

`.cli-flags.toml` is the human-authored source for CLI and process-environment
configuration. `generated/json-schema/env.*.schema.json` files are derivative
runtime-validation witnesses produced from that catalog.

They are **not** independent domain/API contract authorities. When ACT
introduces shared serialized domain, API, HTTP, RPC, event, persistence, IPC, or
durable-storage contracts, TypeSpec and JSON Schema/OpenAPI must be independent,
human-authored peer authorities outside `generated/`. Neither may overwrite the
other. Translations are comparison evidence only; an unexplained mismatch is
`STOPPED_FOR_EVALUATION`.

## Regenerate and verify

The pinned generator revision and exact command are recorded in
`generated/manifest.json`.

```sh
bash scripts/generate-config.sh
python3 scripts/generated_guard.py check --root . --require-manifest --require-frozen
git diff --exit-code -- generated/
```

The script deliberately removes stale Dart, Gleam, and TypeScript projections
before running Rust-only generation.

## Read-only policy

After generation, files are frozen without write bits and directories are
frozen while idle. Git does not persist Unix write bits, so this local safeguard
must be restored after checkout or regeneration. Deterministic regeneration and
the clean-diff CI gate are the durable controls.
