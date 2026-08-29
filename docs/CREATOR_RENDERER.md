# Native creator renderer

The `act-render` binary turns a reviewed creator project into native MP4
masters, variations, and 30–50 second clips. It is the headless rendering core
for the Qt desktop studio; media frames remain inside Rust, FFmpeg, and native
files and never cross the QML bridge.

## Contract and trust boundary

The renderer consumes `act-interfaces` v1 at the pinned merge commit
`0a0a40af581adb89e8a8571afdf61cd8e66d9234`. A project is JSON control-plane
metadata only. It cannot contain a shell command, an FFmpeg filter expression,
a provider token, or media bytes.

Before a process starts, Rust verifies:

- the exact `@anticaptrad` handle and immutable YouTube channel ID
  `UC-Gloecwemo_Mh-VAjnUipg`;
- private visibility, `allowPublic: false`, and confirmed publication rights;
- every asset's normalized relative path, project-root containment, SHA-256,
  owner approval, and license provenance for stock media and sound cues;
- a continuous, bounded timeline with typed segments, motion, transitions,
  captions, audio gain, and loudness targets;
- exact output aspect ratios and dimensions;
- an in-bounds source window and a duration from 30 through 50 seconds for
  every clip; and
- destinations that do not already exist.

FFmpeg is invoked directly with an argument vector. No shell interprets a
project value. Final files are hard-linked atomically from a render workspace
inside the project root, so an existing destination is never replaced and a
cross-filesystem partial copy cannot be mistaken for a completed render.

## Editing stages

The initial native pipeline supports:

1. camera, licensed stock-video, and licensed stock-image segments;
2. generated title cards and text interstitials;
3. static, pan-left, pan-right, tilt-up, tilt-down, zoom-in, and zoom-out motion;
4. cut, fade, and true FFmpeg `xfade` dissolve transitions while preserving the
   declared timeline duration;
5. per-segment gain, delayed sound cues, AAC mixdown, and EBU-style loudness
   normalization;
6. measured, anti-aliased title/caption layout rendered in Rust with the native
   system sans-serif face, plus a deterministic bitmap fallback when no system
   font is available; neither path depends on FFmpeg `drawtext` or `libass`;
7. landscape, portrait, and square H.264/AAC outputs; and
8. FFprobe dimensions/codecs/duration plus SHA-256 and byte size in the render
   receipt.

`faceAware` crop is rejected rather than silently treated as a centered crop.
A later detector-backed implementation must identify and track a subject before
that typed effect becomes eligible. Public publication is also deliberately
outside this process: an approved receipt can become eligible for a **private**
upload, while `publicEligible` remains false.

## Command line

```sh
cargo run --bin act-render -- \
  /absolute/project-root/project.json \
  /absolute/project-root \
  /absolute/project-root/render-receipt.json
```

The receipt argument is optional and defaults to `render-receipt.json` in the
project root. Receipt files and media outputs are create-only. Choose new paths
or explicitly archive prior results before another render.

The reviewed contract example lives in
`act-interfaces/examples/v1/creator-media-project.json`; its placeholder asset
digests must be replaced with real local SHA-256 values before use.

## Verification

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

The integration test synthesizes a 30-second camera recording and a sound cue,
renders a titled/captioned 32-second landscape master plus a 30-second portrait
clip, verifies the private receipt, and proves that a rerun cannot overwrite the
outputs. CI installs FFmpeg and sets `ACT_REQUIRE_FFMPEG_TEST=1`, so this lifecycle
cannot be skipped in the protected test job.
