# AntiCapTrad native desktop studio

`act-desktop-app.rs` is the native, high-throughput desktop application for the
AntiCapTrad publishing stack. It is a first-class product alongside
[`act-flutter`](https://github.com/anticaptrad/act-flutter): neither application
is a wrapper around the other, and both are expected to move the platform
forward independently.

## Why Qt Quick + Rust

The UI is Qt Quick/QML rather than Rust markup or a browser view. Qt provides a
mature native scene graph, GPU rendering, platform accessibility, windowing,
input, and media surfaces. Rust owns the latency-sensitive and security-sensitive
work through a narrow, typed CXX-Qt boundary:

- Tokio task supervision, UDP sockets, signaling, retries, and cancellation;
- WebRTC peer connections through `webrtc-rs`;
- bounded audio, video, and control queues with explicit backpressure;
- stream orchestration and authenticated AntiCapTrad API calls;
- persistence, observability, and secret-safe configuration.

QML receives state, commands, metadata, and opaque native handles. Raw media
frames are never serialized through QML. This division keeps the UI expressive
without turning FFI into the media data plane.

## Current vertical slice

The initial application includes:

- a runnable Qt Quick studio shell for YouTube, Twitch, Rumble, StreamYard, and X;
- a generated, statically typed QML/Rust bridge using CXX-Qt;
- a dedicated multi-threaded Tokio media runtime;
- a real UDP loopback diagnostic callable from the UI;
- validated signaling endpoint and real-time queue-budget domain types;
- `webrtc-rs` linked with its Tokio runtime backend;
- unit tests and strict Clippy gates.

Destination credentials are intentionally not accepted by this UI slice. They
belong in the platform's server-side secret store and should be exposed through
short-lived, least-privilege session grants.

## Development

Requirements:

- Rust 1.88 or newer;
- Qt 6 with Base, Declarative/QML, and Multimedia development packages;
- a C++17-capable compiler.

On macOS with Homebrew:

```sh
brew install qtbase qtdeclarative qtmultimedia
cargo run
```

Quality gates:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

## Architecture guardrails

- Rust owns domain state, authorization decisions, concurrency, and lifecycle.
- QML owns presentation, accessibility, animation, input, and window layout.
- The bridge uses typed properties and commands, not generic maps or JSON blobs.
- Video queues remain small and drop stale frames instead of growing unbounded.
- Tokens, stream keys, and provider credentials must never enter source control,
  URLs, logs, screenshots, QML source, or command-line arguments.
- Remote signaling requires HTTPS or WSS; cleartext is loopback-only.

See [`docs/DESKTOP_TOOLKIT.md`](docs/DESKTOP_TOOLKIT.md) for the framework
decision and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the media and
signaling boundaries.

## Upstream documentation

- [CXX-Qt book](https://kdab.github.io/cxx-qt/book/)
- [Qt Quick scene graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html)
- [Qt Multimedia](https://doc.qt.io/qt-6/qtmultimedia-index.html)
- [webrtc-rs](https://github.com/webrtc-rs/webrtc)

Licensed under the MIT License.
