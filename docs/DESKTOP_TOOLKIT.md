# Desktop toolkit decision

Status: accepted, 2026-08-22.

## Decision

Use Qt 6 Quick/QML for the desktop presentation layer and CXX-Qt 0.9 for a
narrow, statically typed bridge into Rust. Rust uses Tokio for asynchronous
orchestration and `webrtc-rs` for WebRTC primitives.

This follows the same portfolio pattern used by the native Qt applications in
the Daedalus Fab and StreemPilot families while giving AntiCapTrad an explicitly
media-oriented boundary.

## Selection criteria

The desktop application needs native GPU rendering, good accessibility and
window-system integration, mature audio/video surfaces, low-overhead UDP and
WebRTC coordination, and a non-Rust UI language. It must remain viable across
macOS, Windows, and Linux.

Qt Quick is the strongest fit because its scene graph is hardware accelerated,
its QML layer is designed for UI composition, and Qt Multimedia can interoperate
with native frame and audio paths. CXX-Qt generates the C++ glue and retains
typed Rust APIs on the Rust side.

## Alternatives considered

| Toolkit | Strengths | Reason not selected |
| --- | --- | --- |
| Tauri/WebView | Small shell, broad web talent pool | Browser/WebView rendering and IPC are not the desired native media boundary |
| egui/eframe | Productive all-Rust immediate-mode UI | Conflicts with the explicit non-Rust UI requirement and is less aligned with platform-native media surfaces |
| Slint | Rust-friendly declarative UI and modest footprint | Smaller native media ecosystem and less direct Qt Multimedia integration |
| Flutter + Rust bridge | Excellent multi-platform product UI | Already exists as the independent `act-flutter` product; duplicating it would erase the intended architectural diversity |
| GTK/libadwaita | Strong Linux-native experience | Weaker consistency and operational reach across macOS and Windows |

## Boundary rules

The boundary is deliberately asymmetric:

1. QML sends typed user intent such as start, stop, select source, and run a
   transport diagnostic.
2. Rust validates intent and owns state transitions, cancellation, retries,
   credentials, provider calls, and observability.
3. Rust publishes small immutable snapshots and typed property changes.
4. Media frames move through native surfaces, GPU resources, or bounded Rust
   queues. They do not become QVariant maps, JSON, or base64 at the bridge.

Every queue has a capacity and an overload policy. Video prefers recency and may
drop stale frames. Audio protects playout continuity with a bounded jitter
budget. Control messages are loss-intolerant within a bounded queue and surface
backpressure to the caller.

## Consequences

The project takes a Qt build dependency and must maintain a small amount of QML
and generated C++ integration. In exchange it gets native rendering, mature
platform integration, and a media path that can be optimized without replacing
the application shell.
