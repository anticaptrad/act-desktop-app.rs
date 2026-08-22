# Native studio architecture

## Runtime topology

```text
Qt main thread
  QML controls and native media surfaces
            │ typed properties / commands / handles
            ▼
        CXX-Qt bridge
            │
            ▼
Rust control plane
  session state · auth grants · provider orchestration · telemetry
            │ bounded channels
            ▼
Tokio media runtime
  signaling · ICE/STUN/TURN · UDP · WebRTC · capture/encode coordination
            │
            ├── act-api-server.rs
            ├── YouTube / Twitch / Rumble provider adapters
            └── native capture, codec, and output surfaces
```

The Qt event loop is never used as an async executor. Rust work is never allowed
to update QML from an arbitrary runtime thread; results return through CXX-Qt's
Qt-thread queue or typed property setters.

## Media data plane

The control bridge is not a video pipe. The planned media layers are:

- capture adapters that produce timestamped native frames;
- bounded queues with a default maximum of three video frames;
- codec workers that prefer native hardware acceleration where available;
- WebRTC tracks for interactive preview and contribution feeds;
- RTMP/RTMPS or provider-native egress coordinated by server-side grants;
- opaque texture or frame handles connected directly to Qt media/render nodes.

A slow preview must not stall ingest or egress. A slow output receives an
explicit degraded/recovering state and may drop video frames according to policy.

## WebRTC and signaling

`webrtc-rs` is the selected Rust WebRTC implementation. Tokio owns timers,
sockets, cancellation, and signaling I/O. Signaling endpoints must use WSS/HTTPS
outside loopback development, may not embed credentials, and may not include URL
fragments.

Session access tokens are short-lived and provider-neutral. Long-lived provider
refresh tokens and stream keys remain server-side. ICE configuration is obtained
for a session and is not persisted in QML.

## Coexistence with `act-flutter`

The native and Flutter products share API contracts and behavioral expectations,
not a mandatory UI implementation. A future reusable Rust media crate may be
consumed by both through CXX-Qt and `flutter_rust_bridge`, but only after its
boundary is stable and independently versioned. Neither application waits for
the other to ship platform-specific advances.

## Near-term milestones

1. Replace the destination placeholders with read-only provider capability and
   connection state from `act-api-server.rs`.
2. Add cancellable signaling sessions and a synthetic WebRTC loopback test.
3. Add native camera/screen capture and preview without frame serialization.
4. Add server-authorized start/stop orchestration with idempotency keys.
5. Package signed macOS, Windows, and Linux builds with SBOMs and provenance.
