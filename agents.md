# act-desktop-app.rs agent instructions

## Product and architecture invariants

- This is the native Rust desktop companion to `anticaptrad/act-flutter`; both are first-class products and advance independently while preserving semantic product contracts.
- Qt Quick/QML owns presentation, accessibility, windowing, and platform/media surfaces. Rust owns authorization, validation, persistence, Tokio concurrency, UDP/WebRTC sessions, signaling policy, and security-sensitive state.
- Cross the CXX-Qt boundary with narrow typed values. Do not expose broad `QObject` graphs, untyped `QVariant` maps, raw credentials, arbitrary filesystem access, or general process/network capabilities to QML.
- Media payloads must not be serialized through QML. Use bounded queues and native/shared buffer handles for frame and audio data paths; the bridge carries control-plane state and metadata only.
- Embedded browsers and remotely supplied UI are prohibited. OAuth uses the external system browser and short-lived, validated handoff state.
- Never commit API keys, OAuth tokens, TURN credentials, stream keys, signing material, or raw platform responses. Configuration comes from process environment or the OS credential vault; `dotenv` is prohibited.
- Preserve bounded shutdown, redirect rejection, explicit timeouts, private-by-default publishing, and exact `@anticaptrad` channel/account checks.

## Repository safety

- Inspect status, current branch, remotes, default branch, related contracts, and the Flutter companion before editing.
- Preserve unfamiliar or uncommitted work. Never use `git stash`, `git reset`, `git clean`, history rewriting, force pushes, destructive checkout/restore, or bulk deletion.
- Use additive branches and ordinary commits. Resolve conflicts semantically with full surrounding context and scan for conflict markers after resolution.

## Required validation

Run formatting, locked checks, strict Clippy, tests, and a release build. UI or media claims additionally require native macOS, Windows, and Linux build/package evidence plus focused device tests.

