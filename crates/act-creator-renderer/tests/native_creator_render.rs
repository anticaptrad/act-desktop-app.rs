use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use act_creator_renderer::{NativeRenderer, RendererConfig};
use act_interfaces::creator_media::{OutputKind, ReceiptReviewState, RenderStatus};
use sha2::{Digest, Sha256};

#[test]
fn native_renderer_proves_master_clip_and_private_receipt_lifecycle() {
    if !tool_available("ffmpeg") || !tool_available("ffprobe") {
        assert!(
            std::env::var_os("ACT_REQUIRE_FFMPEG_TEST").is_none(),
            "FFmpeg is required by the CI lifecycle gate"
        );
        eprintln!("skipping native render lifecycle because FFmpeg is unavailable");
        return;
    }

    let root = tempfile::tempdir().expect("fixture root should be created");
    let assets = root.path().join("assets");
    fs::create_dir(&assets).expect("asset directory should be created");
    let camera = assets.join("camera.mp4");
    let cue = assets.join("cue.wav");
    generate_camera_fixture(&camera);
    generate_cue_fixture(&cue);
    let project_path = write_project_fixture(root.path(), &camera, &cue);

    let renderer = NativeRenderer::new(RendererConfig {
        command_timeout: Duration::from_secs(5 * 60),
        ..RendererConfig::default()
    });
    let receipt = renderer
        .render(&project_path, root.path())
        .expect("native lifecycle render should succeed");

    assert_eq!(receipt.status, RenderStatus::Succeeded);
    assert_eq!(receipt.review_state, ReceiptReviewState::Approved);
    assert!(receipt.publication.private_upload_eligible);
    assert!(!receipt.publication.public_eligible);
    assert_eq!(receipt.outputs.len(), 2);
    assert!(receipt.outputs.iter().any(|output| {
        output.kind == OutputKind::Clip
            && (30_000..=50_000).contains(&output.duration_ms)
            && output.width == 360
            && output.height == 640
    }));
    assert!(root.path().join("outputs/master-landscape.mp4").is_file());
    assert!(root.path().join("outputs/clip-vertical.mp4").is_file());
    capture_previews_if_requested(root.path());

    let rerun = renderer.render(&project_path, root.path());
    assert!(rerun.is_err(), "renderer must never overwrite outputs");
}

fn write_project_fixture(root: &Path, camera: &Path, cue: &Path) -> PathBuf {
    let project_path = root.join("project.json");
    let project = include_str!("fixtures/native-creator-project.json")
        .replace("__CAMERA_SHA256__", &sha256(camera))
        .replace("__CUE_SHA256__", &sha256(cue));
    serde_json::from_str::<serde_json::Value>(&project).expect("fixture JSON should be valid");
    fs::write(&project_path, project).expect("project fixture should be written");
    project_path
}

fn capture_previews_if_requested(root: &Path) {
    let Some(preview_root) = std::env::var_os("ACT_CAPTURE_RENDER_PREVIEW") else {
        return;
    };
    let preview_root = PathBuf::from(preview_root);
    fs::create_dir_all(&preview_root).expect("preview directory should be created");
    capture_frame(
        &root.join("outputs/master-landscape.mp4"),
        "0.5",
        &preview_root.join("opening-title.png"),
    );
    capture_frame(
        &root.join("outputs/master-landscape.mp4"),
        "2",
        &preview_root.join("camera-caption.png"),
    );
    capture_frame(
        &root.join("outputs/clip-vertical.mp4"),
        "2",
        &preview_root.join("vertical-clip.png"),
    );
}

fn capture_frame(video: &Path, timestamp: &str, output: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-ss",
        ])
        .arg(timestamp)
        .args(["-i"])
        .arg(video)
        .args(["-frames:v", "1"])
        .arg(output)
        .status()
        .expect("preview FFmpeg should start");
    assert!(status.success(), "preview frame should render");
}

fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn generate_camera_fixture(path: &Path) {
    run_ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=30:size=640x360:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:sample_rate=48000:duration=30",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "28",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
        path,
    );
}

fn generate_cue_fixture(path: &Path) {
    run_ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=0.4",
            "-c:a",
            "pcm_s16le",
        ],
        path,
    );
}

fn run_ffmpeg(args: &[&str], path: &Path) {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .arg(path)
        .status()
        .expect("fixture FFmpeg should start");
    assert!(status.success(), "fixture should render");
}

fn sha256(path: &Path) -> String {
    let digest = Sha256::digest(fs::read(path).expect("fixture bytes should be readable"));
    digest.iter().fold(String::new(), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
        output
    })
}
