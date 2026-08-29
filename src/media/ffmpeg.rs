use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use act_interfaces::creator_media::{
    AssetKind, AudioCodec, CreatorRenderReceipt, Crop, MediaType, Motion, OutputPlan,
    OutputReceipt, TextPosition, TimelineSegment, TransitionKind, VideoCodec,
};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use fontdb::{Database, Family, Query};
use fontdue::Font;
use fontdue::layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle};
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use tempfile::{Builder, TempDir};
use wait_timeout::ChildExt;

use super::project::{Canvas, ResolvedAsset, ValidatedProject, sha256_file};
use super::receipt::successful_receipt;
use super::{RenderError, RendererConfig};

const FRAME_RATE: u32 = 30;
const MAX_CAPTURE_BYTES: u64 = 2 * 1024 * 1024;
const PROBE_DURATION_TOLERANCE_MS: u64 = 750;

#[derive(Clone, Debug)]
pub(super) struct FfmpegEngine {
    config: RendererConfig,
}

#[derive(Debug)]
struct SegmentArtifact {
    path: PathBuf,
    duration_ms: u64,
}

#[derive(Debug)]
struct PendingOutput {
    rendered: PathBuf,
    destination: PathBuf,
    receipt: OutputReceipt,
}

#[derive(Debug)]
struct CaptionOverlay {
    path: PathBuf,
    start_ms: u64,
    end_ms: u64,
}

#[derive(Clone, Copy, Debug)]
enum TextRole {
    Title,
    Caption,
}

#[derive(Clone, Copy, Debug)]
struct TextPresentation {
    canvas: Canvas,
    position: TextPosition,
    role: TextRole,
    color: [u8; 3],
    opaque_background: bool,
}

#[derive(Debug)]
struct SegmentInputs {
    args: Vec<OsString>,
    external_overlay: bool,
    audio_map: String,
}

impl FfmpegEngine {
    pub const fn new(config: RendererConfig) -> Self {
        Self { config }
    }

    pub fn render(&self, project: &ValidatedProject) -> Result<CreatorRenderReceipt, RenderError> {
        let ffmpeg_version = self.ffmpeg_version()?;
        self.ensure_ffprobe()?;
        let workspace = Builder::new()
            .prefix(".act-render-")
            .tempdir_in(&project.root)
            .map_err(|error| {
                RenderError::RenderFailed(format!("could not create render workspace: {error}"))
            })?;

        let segments = self.render_segments(project, &workspace)?;
        let assembled = self.assemble_timeline(project, &workspace, &segments)?;
        let embellished = self.mix_audio_cues(project, &workspace, &assembled)?;

        let mut pending = Vec::with_capacity(project.project.outputs.len());
        for (index, output) in project.project.outputs.iter().enumerate() {
            let rendered = self.render_output(project, &workspace, &embellished, output, index)?;
            let receipt = self.probe_output(output, &rendered)?;
            pending.push(PendingOutput {
                rendered,
                destination: project.outputs[&output.output_id].clone(),
                receipt,
            });
        }

        Self::verify_asset_digests(project)?;
        Self::publish_outputs(project, &pending)?;
        Ok(successful_receipt(
            project,
            ffmpeg_version,
            pending.into_iter().map(|output| output.receipt).collect(),
        ))
    }

    fn ffmpeg_version(&self) -> Result<String, RenderError> {
        let output = self
            .run_capture(
                &self.config.ffmpeg,
                [OsStr::new("-version")],
                "ffmpeg version",
            )
            .map_err(|_| {
                RenderError::RendererUnavailable("ffmpeg could not report its version".into())
            })?;
        let text = String::from_utf8_lossy(&output);
        let version = text.lines().next().unwrap_or_default().trim();
        if version.is_empty() {
            return Err(RenderError::RendererUnavailable(
                "ffmpeg returned an empty version".into(),
            ));
        }
        Ok(version.chars().take(200).collect())
    }

    fn ensure_ffprobe(&self) -> Result<(), RenderError> {
        self.run_capture(
            &self.config.ffprobe,
            [OsStr::new("-version")],
            "ffprobe version",
        )
        .map(|_| ())
        .map_err(|_| {
            RenderError::RendererUnavailable("ffprobe could not report its version".into())
        })
    }

    fn render_segments(
        &self,
        project: &ValidatedProject,
        workspace: &TempDir,
    ) -> Result<Vec<SegmentArtifact>, RenderError> {
        project
            .project
            .timeline
            .iter()
            .enumerate()
            .map(|(index, segment)| self.render_segment(project, workspace, segment, index))
            .collect()
    }

    fn render_segment(
        &self,
        project: &ValidatedProject,
        workspace: &TempDir,
        segment: &TimelineSegment,
        index: usize,
    ) -> Result<SegmentArtifact, RenderError> {
        let destination = workspace.path().join(format!("segment-{index:04}.mp4"));
        let SegmentInputs {
            mut args,
            external_overlay,
            audio_map,
        } = self.prepare_segment_inputs(project, workspace, segment, index)?;
        let video_filter = segment_video_filter(segment, project.canvas, index == 0);
        let audio_filter = segment_audio_filter(segment, index == 0);
        extend_args(&mut args, ["-t", &seconds(segment.duration_ms)]);
        if external_overlay {
            let complex_filter = format!(
                "[0:v:0]{}[base];[1:v:0]format=rgba[copy];[base][copy]overlay=0:0{}[v]",
                motion_filter(segment, project.canvas),
                initial_video_fade(segment, index == 0)
            );
            extend_args(
                &mut args,
                ["-filter_complex", &complex_filter, "-map", "[v]"],
            );
        } else {
            extend_args(&mut args, ["-map", "0:v:0", "-vf", &video_filter]);
        }
        extend_args(&mut args, ["-map", &audio_map, "-af", &audio_filter]);
        append_segment_encoding(&mut args);
        args.push(destination.as_os_str().to_owned());
        self.run_status(
            &self.config.ffmpeg,
            &args,
            &format!("segment {}", segment.segment_id),
        )?;

        Ok(SegmentArtifact {
            path: destination,
            duration_ms: segment.duration_ms,
        })
    }

    fn prepare_segment_inputs(
        &self,
        project: &ValidatedProject,
        workspace: &TempDir,
        segment: &TimelineSegment,
        index: usize,
    ) -> Result<SegmentInputs, RenderError> {
        let asset_contract = segment.asset_id.as_ref().and_then(|asset_id| {
            project
                .project
                .assets
                .iter()
                .find(|asset| &asset.asset_id == asset_id)
        });
        let resolved = segment
            .asset_id
            .as_ref()
            .and_then(|asset_id| project.assets.get(asset_id));
        let asset_is_image = asset_contract.is_some_and(|asset| {
            asset.kind == AssetKind::StockImage
                || matches!(asset.media_type, MediaType::ImagePng | MediaType::ImageJpeg)
        });
        let text_overlay = write_segment_overlay(
            workspace,
            segment,
            index,
            project.canvas,
            resolved.is_none(),
        )?;
        let has_source_audio = if let (Some(asset), false) = (resolved, asset_is_image) {
            self.probe_has_audio(asset, &segment.segment_id)?
        } else {
            false
        };

        let mut args = base_args();
        if let Some(asset) = resolved {
            if asset_is_image {
                extend_args(&mut args, ["-loop", "1", "-framerate", "30", "-i"]);
                args.push(asset.path.as_os_str().to_owned());
            } else {
                extend_args(
                    &mut args,
                    ["-ss", &seconds(segment.source_in_ms.unwrap_or(0)), "-i"],
                );
                args.push(asset.path.as_os_str().to_owned());
            }
        } else if let Some(overlay) = &text_overlay {
            extend_args(&mut args, ["-loop", "1", "-framerate", "30", "-i"]);
            args.push(overlay.as_os_str().to_owned());
        } else {
            let color = format!(
                "color=c=0x101820:s={}x{}:r={FRAME_RATE}:d={}",
                project.canvas.width,
                project.canvas.height,
                seconds(segment.duration_ms)
            );
            extend_args(&mut args, ["-f", "lavfi", "-i", &color]);
        }

        let external_overlay = resolved
            .is_some()
            .then_some(text_overlay.as_ref())
            .flatten();
        let mut next_input_index = 1_usize;
        if let Some(overlay) = external_overlay {
            extend_args(&mut args, ["-loop", "1", "-framerate", "30", "-i"]);
            args.push(overlay.as_os_str().to_owned());
            next_input_index += 1;
        }
        if !has_source_audio {
            extend_args(
                &mut args,
                [
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=channel_layout=stereo:sample_rate=48000",
                ],
            );
        }

        let audio_map = if has_source_audio {
            "0:a:0".to_owned()
        } else {
            format!("{next_input_index}:a:0")
        };
        Ok(SegmentInputs {
            args,
            external_overlay: external_overlay.is_some(),
            audio_map,
        })
    }

    fn probe_has_audio(
        &self,
        asset: &ResolvedAsset,
        segment_id: &str,
    ) -> Result<bool, RenderError> {
        let args = vec![
            OsString::from("-v"),
            OsString::from("error"),
            OsString::from("-select_streams"),
            OsString::from("a:0"),
            OsString::from("-show_entries"),
            OsString::from("stream=index"),
            OsString::from("-of"),
            OsString::from("csv=p=0"),
            asset.path.as_os_str().to_owned(),
        ];
        let output = self
            .run_capture(&self.config.ffprobe, &args, "audio stream probe")
            .map_err(|_| RenderError::ProbeFailed(format!("segment {segment_id}")))?;
        Ok(!String::from_utf8_lossy(&output).trim().is_empty())
    }

    fn assemble_timeline(
        &self,
        project: &ValidatedProject,
        workspace: &TempDir,
        segments: &[SegmentArtifact],
    ) -> Result<PathBuf, RenderError> {
        if segments.len() == 1 {
            return Ok(segments[0].path.clone());
        }

        let destination = workspace.path().join("assembly.mp4");
        let mut args = base_args();
        for segment in segments {
            extend_args(&mut args, ["-i"]);
            args.push(segment.path.as_os_str().to_owned());
        }
        let mut filter =
            String::from("[0:v:0]setpts=PTS-STARTPTS[v0];[0:a:0]asetpts=PTS-STARTPTS[a0];");
        let mut cumulative_ms = segments[0].duration_ms;
        for (index, segment) in segments.iter().enumerate().skip(1) {
            let transition = &project.project.timeline[index].transition;
            let previous = index - 1;
            write!(
                &mut filter,
                "[{index}:v:0]setpts=PTS-STARTPTS[vin{index}];\
                 [{index}:a:0]asetpts=PTS-STARTPTS[ain{index}];"
            )
            .expect("writing to String cannot fail");
            if transition.kind == TransitionKind::Cut {
                write!(
                    &mut filter,
                    "[v{previous}][a{previous}][vin{index}][ain{index}]\
                     concat=n=2:v=1:a=1[v{index}][a{index}];"
                )
                .expect("writing to String cannot fail");
            } else {
                let duration = seconds(transition.duration_ms);
                let offset = seconds(cumulative_ms);
                let transition_name = match transition.kind {
                    TransitionKind::Fade => "fade",
                    TransitionKind::Dissolve => "dissolve",
                    TransitionKind::Cut => unreachable!(),
                };
                write!(
                    &mut filter,
                    "[v{previous}]tpad=stop_mode=clone:stop_duration={duration}[vpad{index}];\
                     [vpad{index}][vin{index}]xfade=transition={transition_name}:duration={duration}:offset={offset}[v{index}];\
                     [a{previous}]apad=pad_dur={duration}[apad{index}];\
                     [apad{index}][ain{index}]acrossfade=d={duration}:c1=tri:c2=tri[a{index}];"
                )
                .expect("writing to String cannot fail");
            }
            cumulative_ms += segment.duration_ms;
        }
        let last = segments.len() - 1;
        extend_args(
            &mut args,
            [
                "-filter_complex",
                &filter,
                "-map",
                &format!("[v{last}]"),
                "-map",
                &format!("[a{last}]"),
                "-t",
                &seconds(cumulative_ms),
            ],
        );
        append_segment_encoding(&mut args);
        args.push(destination.as_os_str().to_owned());
        self.run_status(&self.config.ffmpeg, &args, "single-pass timeline assembly")?;
        Ok(destination)
    }

    fn mix_audio_cues(
        &self,
        project: &ValidatedProject,
        workspace: &TempDir,
        assembled: &Path,
    ) -> Result<PathBuf, RenderError> {
        if project.project.audio.cues.is_empty() {
            return Ok(assembled.to_path_buf());
        }

        let destination = workspace.path().join("embellished.mp4");
        let mut args = base_args();
        extend_args(&mut args, ["-i"]);
        args.push(assembled.as_os_str().to_owned());
        for cue in &project.project.audio.cues {
            extend_args(&mut args, ["-i"]);
            args.push(project.assets[&cue.asset_id].path.as_os_str().to_owned());
        }

        let mut filter = String::from("[0:a:0]anull[base];");
        let mut mix_inputs = String::from("[base]");
        for (index, cue) in project.project.audio.cues.iter().enumerate() {
            let input = index + 1;
            write!(
                &mut filter,
                "[{input}:a:0]adelay={}:all=1,volume={}dB[cue{index}];",
                cue.start_ms, cue.gain_db
            )
            .expect("writing to String cannot fail");
            write!(&mut mix_inputs, "[cue{index}]").expect("writing to String cannot fail");
        }
        write!(
            &mut filter,
            "{mix_inputs}amix=inputs={}:duration=first:dropout_transition=0[a]",
            project.project.audio.cues.len() + 1
        )
        .expect("writing to String cannot fail");
        extend_args(
            &mut args,
            [
                "-filter_complex",
                &filter,
                "-map",
                "0:v:0",
                "-map",
                "[a]",
                "-t",
                &seconds(project.timeline_duration_ms),
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-movflags",
                "+faststart",
            ],
        );
        args.push(destination.as_os_str().to_owned());
        self.run_status(&self.config.ffmpeg, &args, "audio cue mix")?;
        Ok(destination)
    }

    fn render_output(
        &self,
        project: &ValidatedProject,
        workspace: &TempDir,
        assembled: &Path,
        output: &OutputPlan,
        index: usize,
    ) -> Result<PathBuf, RenderError> {
        let destination = workspace.path().join(format!("output-{index:03}.mp4"));
        let mut args = base_args();
        let window_start_ms = output
            .source_window
            .as_ref()
            .map_or(0, |window| window.start_ms);
        extend_args(&mut args, ["-ss", &seconds(window_start_ms), "-i"]);
        args.push(assembled.as_os_str().to_owned());

        let captions = write_output_captions(project, workspace, output, index)?;
        for caption in &captions {
            extend_args(&mut args, ["-loop", "1", "-framerate", "30", "-i"]);
            args.push(caption.path.as_os_str().to_owned());
        }
        let video_filter = format!(
            "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},setsar=1",
            output.width, output.height, output.width, output.height
        );
        let audio_filter = format!(
            "loudnorm=I={}:TP={}:LRA=11",
            project.project.audio.target_lufs, project.project.audio.true_peak_db
        );
        extend_args(&mut args, ["-t", &seconds(output.duration_ms)]);
        if captions.is_empty() {
            extend_args(&mut args, ["-map", "0:v:0", "-vf", &video_filter]);
        } else {
            let mut complex = format!("[0:v:0]{video_filter}[base];");
            let mut previous = "base".to_owned();
            for (caption_index, caption) in captions.iter().enumerate() {
                let input = caption_index + 1;
                let output_label = if caption_index + 1 == captions.len() {
                    "v".to_owned()
                } else {
                    format!("v{caption_index}")
                };
                write!(
                    &mut complex,
                    "[{input}:v:0]format=rgba[caption{caption_index}];\
                     [{previous}][caption{caption_index}]overlay=0:0:enable='between(t,{},{})'[{output_label}];",
                    seconds(caption.start_ms),
                    seconds(caption.end_ms)
                )
                .expect("writing to String cannot fail");
                previous = output_label;
            }
            extend_args(&mut args, ["-filter_complex", &complex, "-map", "[v]"]);
        }
        extend_args(
            &mut args,
            [
                "-map",
                "0:a:0",
                "-af",
                &audio_filter,
                "-r",
                "30",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-movflags",
                "+faststart",
            ],
        );
        args.push(destination.as_os_str().to_owned());
        self.run_status(
            &self.config.ffmpeg,
            &args,
            &format!("output {}", output.output_id),
        )?;
        Ok(destination)
    }

    fn probe_output(
        &self,
        output: &OutputPlan,
        rendered: &Path,
    ) -> Result<OutputReceipt, RenderError> {
        let args = vec![
            OsString::from("-v"),
            OsString::from("error"),
            OsString::from("-show_entries"),
            OsString::from("format=duration:stream=codec_type,codec_name,width,height"),
            OsString::from("-of"),
            OsString::from("json"),
            rendered.as_os_str().to_owned(),
        ];
        let raw = self
            .run_capture(&self.config.ffprobe, &args, "rendered output probe")
            .map_err(|_| RenderError::ProbeFailed(output.output_id.clone()))?;
        let probe: ProbeDocument = serde_json::from_slice(&raw)
            .map_err(|_| RenderError::ProbeFailed(output.output_id.clone()))?;
        let video = probe
            .streams
            .iter()
            .find(|stream| stream.codec_type.as_deref() == Some("video"))
            .ok_or_else(|| RenderError::ProbeFailed(output.output_id.clone()))?;
        let audio = probe
            .streams
            .iter()
            .find(|stream| stream.codec_type.as_deref() == Some("audio"))
            .ok_or_else(|| RenderError::ProbeFailed(output.output_id.clone()))?;
        let duration_ms = probe
            .format
            .duration
            .parse::<f64>()
            .ok()
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .and_then(|duration| {
                u64::try_from(std::time::Duration::from_secs_f64(duration).as_millis()).ok()
            })
            .ok_or_else(|| RenderError::ProbeFailed(output.output_id.clone()))?;
        if duration_ms.abs_diff(output.duration_ms) > PROBE_DURATION_TOLERANCE_MS
            || video.width != Some(output.width)
            || video.height != Some(output.height)
            || video.codec_name.as_deref() != Some("h264")
            || audio.codec_name.as_deref() != Some("aac")
        {
            return Err(RenderError::ProbeFailed(format!(
                "{} does not match its declared media contract",
                output.output_id
            )));
        }
        let metadata = rendered
            .metadata()
            .map_err(|_| RenderError::ProbeFailed(output.output_id.clone()))?;
        if metadata.len() == 0 {
            return Err(RenderError::ProbeFailed(format!(
                "{} is empty",
                output.output_id
            )));
        }
        let sha256 = sha256_file(rendered)
            .map_err(|_| RenderError::ProbeFailed(output.output_id.clone()))?;

        Ok(OutputReceipt {
            output_id: output.output_id.clone(),
            kind: output.kind,
            relative_path: output.relative_path.clone(),
            sha256,
            size_bytes: metadata.len(),
            duration_ms,
            width: output.width,
            height: output.height,
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
            source_window: output.source_window.clone(),
        })
    }

    fn publish_outputs(
        project: &ValidatedProject,
        pending: &[PendingOutput],
    ) -> Result<(), RenderError> {
        for output in pending {
            if output.destination.exists() {
                return Err(RenderError::InvalidOutput(format!(
                    "{} appeared during rendering and will not be overwritten",
                    output.receipt.output_id
                )));
            }
            let parent = output.destination.parent().ok_or_else(|| {
                RenderError::InvalidOutput(format!(
                    "{} has no output parent",
                    output.receipt.output_id
                ))
            })?;
            fs::create_dir_all(parent).map_err(|error| {
                RenderError::InvalidOutput(format!(
                    "{} output directory could not be created: {error}",
                    output.receipt.output_id
                ))
            })?;
            let canonical_parent = parent.canonicalize().map_err(|error| {
                RenderError::InvalidOutput(format!(
                    "{} output directory could not be verified: {error}",
                    output.receipt.output_id
                ))
            })?;
            if !canonical_parent.starts_with(&project.root) {
                return Err(RenderError::InvalidOutput(format!(
                    "{} output directory escaped the project root",
                    output.receipt.output_id
                )));
            }
        }
        let mut published = Vec::with_capacity(pending.len());
        for output in pending {
            if let Err(error) = publish_without_overwrite(&output.rendered, &output.destination) {
                for destination in published {
                    let _ = fs::remove_file(destination);
                }
                return Err(RenderError::InvalidOutput(format!(
                    "{} could not be published without overwrite: {error}",
                    output.receipt.output_id
                )));
            }
            published.push(&output.destination);
        }
        Ok(())
    }

    fn verify_asset_digests(project: &ValidatedProject) -> Result<(), RenderError> {
        for asset in &project.project.assets {
            let resolved = &project.assets[&asset.asset_id];
            let current = sha256_file(&resolved.path)
                .map_err(|_| RenderError::AssetNotFound(asset.asset_id.clone()))?;
            if current != resolved.sha256 {
                return Err(RenderError::AssetDigestMismatch(format!(
                    "{} changed during rendering",
                    asset.asset_id
                )));
            }
        }
        Ok(())
    }

    fn run_status(
        &self,
        program: &Path,
        args: &[OsString],
        label: &str,
    ) -> Result<(), RenderError> {
        self.execute(program, args, label).map(|_| ())
    }

    fn run_capture<I, S>(
        &self,
        program: &Path,
        args: I,
        label: &str,
    ) -> Result<Vec<u8>, RenderError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect::<Vec<_>>();
        self.execute(program, &args, label)
    }

    fn execute(
        &self,
        program: &Path,
        args: &[OsString],
        label: &str,
    ) -> Result<Vec<u8>, RenderError> {
        let mut stdout = tempfile::tempfile().map_err(|error| {
            RenderError::RenderFailed(format!("{label} capture could not start: {error}"))
        })?;
        let stderr = tempfile::tempfile().map_err(|error| {
            RenderError::RenderFailed(format!("{label} capture could not start: {error}"))
        })?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone().map_err(|error| {
                RenderError::RenderFailed(format!("{label} capture failed: {error}"))
            })?))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| RenderError::RendererUnavailable(format!("{label}: {error}")))?;
        let status = child
            .wait_timeout(self.config.command_timeout)
            .map_err(|error| RenderError::RenderFailed(format!("{label} wait failed: {error}")))?;
        let Some(status) = status else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RenderError::RenderFailed(format!("{label} timed out")));
        };
        if !status.success() {
            return Err(RenderError::RenderFailed(format!(
                "{label} exited unsuccessfully"
            )));
        }
        stdout.seek(SeekFrom::Start(0)).map_err(|error| {
            RenderError::RenderFailed(format!("{label} output could not be read: {error}"))
        })?;
        let mut output = Vec::new();
        stdout
            .take(MAX_CAPTURE_BYTES)
            .read_to_end(&mut output)
            .map_err(|error| {
                RenderError::RenderFailed(format!("{label} output could not be read: {error}"))
            })?;
        Ok(output)
    }
}

fn base_args() -> Vec<OsString> {
    [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-filter_complex_threads",
        "1",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn append_segment_encoding(args: &mut Vec<OsString>) {
    extend_args(
        args,
        [
            "-r",
            "30",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-shortest",
            "-movflags",
            "+faststart",
        ],
    );
}

fn extend_args<const N: usize>(args: &mut Vec<OsString>, values: [&str; N]) {
    args.extend(values.into_iter().map(OsString::from));
}

fn seconds(milliseconds: u64) -> String {
    format!("{}.{:03}", milliseconds / 1_000, milliseconds % 1_000)
}

fn write_segment_overlay(
    workspace: &TempDir,
    segment: &TimelineSegment,
    index: usize,
    canvas: Canvas,
    opaque_background: bool,
) -> Result<Option<PathBuf>, RenderError> {
    let Some(text) = &segment.text else {
        return Ok(None);
    };
    let path = workspace.path().join(format!("text-{index:04}.png"));
    let mut content = vec![text.heading.trim().to_owned()];
    if let Some(subheading) = &text.subheading {
        content.push(subheading.trim().to_owned());
    }
    render_text_overlay(
        &path,
        canvas,
        &content,
        text.position,
        TextRole::Title,
        parse_hex_color(&text.accent_color),
        opaque_background,
    )
    .map_err(|error| {
        RenderError::RenderFailed(format!(
            "segment {} text could not be prepared: {error}",
            segment.segment_id
        ))
    })?;
    Ok(Some(path))
}

fn segment_video_filter(segment: &TimelineSegment, canvas: Canvas, first: bool) -> String {
    let mut filter = motion_filter(segment, canvas);
    filter.push_str(&initial_video_fade(segment, first));
    filter
}

fn initial_video_fade(segment: &TimelineSegment, first: bool) -> String {
    if first && segment.transition.kind != TransitionKind::Cut {
        format!(
            ",fade=t=in:st=0:d={}",
            seconds(segment.transition.duration_ms)
        )
    } else {
        String::new()
    }
}

fn motion_filter(segment: &TimelineSegment, canvas: Canvas) -> String {
    let width = canvas.width;
    let height = canvas.height;
    let duration = seconds(segment.duration_ms);
    match segment.motion {
        Motion::Static => geometry_filter(segment.crop, width, height),
        Motion::PanLeft | Motion::PanRight | Motion::TiltUp | Motion::TiltDown => {
            let large_width = scale_up_112(width);
            let large_height = scale_up_112(height);
            let (x, y) = match segment.motion {
                Motion::PanLeft => (
                    format!("(in_w-out_w)*(1-min(t/{duration},1))"),
                    "(in_h-out_h)/2".into(),
                ),
                Motion::PanRight => (
                    format!("(in_w-out_w)*min(t/{duration},1)"),
                    "(in_h-out_h)/2".into(),
                ),
                Motion::TiltUp => (
                    "(in_w-out_w)/2".into(),
                    format!("(in_h-out_h)*(1-min(t/{duration},1))"),
                ),
                Motion::TiltDown => (
                    "(in_w-out_w)/2".into(),
                    format!("(in_h-out_h)*min(t/{duration},1)"),
                ),
                _ => unreachable!(),
            };
            format!(
                "scale={large_width}:{large_height}:force_original_aspect_ratio=increase,crop={large_width}:{large_height},crop={width}:{height}:x='{x}':y='{y}',setsar=1"
            )
        }
        Motion::ZoomIn | Motion::ZoomOut => {
            let frames = (segment.duration_ms * u64::from(FRAME_RATE) / 1_000).max(1);
            let zoom = if segment.motion == Motion::ZoomIn {
                format!("min(1.0+0.12*on/{frames},1.12)")
            } else {
                format!("max(1.12-0.12*on/{frames},1.0)")
            };
            format!(
                "{},zoompan=z='{zoom}':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=1:s={width}x{height}:fps={FRAME_RATE},setsar=1",
                geometry_filter(Crop::Fill, width, height)
            )
        }
    }
}

fn geometry_filter(crop: Crop, width: u32, height: u32) -> String {
    match crop {
        Crop::Fit => format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1"
        ),
        Crop::Fill | Crop::FaceAware => format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},setsar=1"
        ),
    }
}

fn segment_audio_filter(segment: &TimelineSegment, first: bool) -> String {
    let mut filter = format!("volume={}dB", segment.audio_gain_db);
    if first && segment.transition.kind != TransitionKind::Cut {
        write!(
            &mut filter,
            ",afade=t=in:st=0:d={}",
            seconds(segment.transition.duration_ms)
        )
        .expect("writing to String cannot fail");
    }
    filter
}

fn scale_up_112(value: u32) -> u32 {
    let rounded = u32::try_from((u64::from(value) * 112).div_ceil(100))
        .expect("bounded video dimensions fit u32");
    rounded + (rounded % 2)
}

fn write_output_captions(
    project: &ValidatedProject,
    workspace: &TempDir,
    output: &OutputPlan,
    index: usize,
) -> Result<Vec<CaptionOverlay>, RenderError> {
    let window_start = output
        .source_window
        .as_ref()
        .map_or(0, |window| window.start_ms);
    let window_end = window_start + output.duration_ms;
    project
        .project
        .captions
        .iter()
        .enumerate()
        .filter_map(|caption| {
            let (caption_index, caption) = caption;
            let start = caption.start_ms.max(window_start);
            let end = caption.end_ms.min(window_end);
            (start < end).then_some((
                caption_index,
                start - window_start,
                end - window_start,
                caption,
            ))
        })
        .map(|(caption_index, start_ms, end_ms, caption)| {
            let path = workspace
                .path()
                .join(format!("caption-{index:03}-{caption_index:03}.png"));
            let text = caption
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            render_text_overlay(
                &path,
                Canvas {
                    width: output.width,
                    height: output.height,
                },
                &[text],
                TextPosition::LowerThird,
                TextRole::Caption,
                [255, 255, 255],
                false,
            )
            .map_err(|error| {
                RenderError::RenderFailed(format!(
                    "output {} caption could not be prepared: {error}",
                    output.output_id
                ))
            })?;
            Ok(CaptionOverlay {
                path,
                start_ms,
                end_ms,
            })
        })
        .collect()
}

fn render_text_overlay(
    path: &Path,
    canvas: Canvas,
    text: &[String],
    position: TextPosition,
    role: TextRole,
    color: [u8; 3],
    opaque_background: bool,
) -> Result<(), image::ImageError> {
    let background = if opaque_background {
        Rgba([16, 24, 32, 255])
    } else {
        Rgba([0, 0, 0, 0])
    };
    let mut image = RgbaImage::from_pixel(canvas.width, canvas.height, background);
    let presentation = TextPresentation {
        canvas,
        position,
        role,
        color,
        opaque_background,
    };
    if let Some(font) = system_sans_serif_font() {
        draw_system_text(&mut image, text, presentation, font);
    } else {
        draw_fallback_text(&mut image, text, presentation);
    }
    image.save(path)
}

fn system_sans_serif_font() -> Option<&'static Font> {
    static SYSTEM_FONT: OnceLock<Option<Font>> = OnceLock::new();
    SYSTEM_FONT
        .get_or_init(|| {
            let mut database = Database::new();
            database.load_system_fonts();
            let face_id = database.query(&Query {
                families: &[Family::SansSerif],
                ..Query::default()
            })?;
            database
                .with_face_data(face_id, |bytes, collection_index| {
                    Font::from_bytes(
                        bytes,
                        fontdue::FontSettings {
                            collection_index,
                            ..fontdue::FontSettings::default()
                        },
                    )
                    .ok()
                })
                .flatten()
        })
        .as_ref()
}

fn draw_system_text(
    image: &mut RgbaImage,
    text: &[String],
    presentation: TextPresentation,
    font: &Font,
) {
    let TextPresentation {
        canvas,
        position,
        role,
        color,
        opaque_background,
    } = presentation;
    let content = text
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if content.is_empty() {
        return;
    }

    let horizontal_margin = match role {
        TextRole::Title => dimension_f32(canvas.width) * 0.10,
        TextRole::Caption => dimension_f32(canvas.width) * 0.07,
    }
    .max(12.0);
    let maximum_width = (dimension_f32(canvas.width) - 2.0 * horizontal_margin).max(1.0);
    let maximum_height = dimension_f32(canvas.height)
        * match role {
            TextRole::Title => 0.65,
            TextRole::Caption => 0.30,
        };
    let minimum_size = match role {
        TextRole::Title => 24.0,
        TextRole::Caption => 20.0,
    };
    let mut font_size = match role {
        TextRole::Title => dimension_f32(canvas.height) * 0.070,
        TextRole::Caption => dimension_f32(canvas.height) * 0.042,
    }
    .clamp(
        minimum_size,
        match role {
            TextRole::Title => 84.0,
            TextRole::Caption => 56.0,
        },
    );

    let mut layout =
        create_text_layout(font, &content, horizontal_margin, maximum_width, font_size);
    while layout.height() > maximum_height && font_size > minimum_size {
        font_size = (font_size - 2.0).max(minimum_size);
        layout = create_text_layout(font, &content, horizontal_margin, maximum_width, font_size);
    }

    let Some((minimum_x, minimum_y, maximum_x, maximum_y)) = glyph_bounds(&layout) else {
        return;
    };
    let text_height = maximum_y - minimum_y;
    let desired_top = text_top(dimension_f32(canvas.height), text_height, position);
    let vertical_offset = desired_top - minimum_y;
    let box_margin = (font_size * 0.45).clamp(10.0, 28.0);
    let box_left = bounded_pixel(minimum_x - box_margin, canvas.width);
    let box_top = bounded_pixel(desired_top - box_margin, canvas.height);
    let box_right = bounded_pixel(maximum_x + box_margin, canvas.width);
    let box_bottom = bounded_pixel(desired_top + text_height + box_margin, canvas.height);
    fill_rectangle(
        image,
        box_left,
        box_top,
        box_right.saturating_sub(box_left),
        box_bottom.saturating_sub(box_top),
        if opaque_background {
            Rgba([4, 8, 12, 224])
        } else {
            Rgba([0, 0, 0, 176])
        },
    );

    for glyph in layout.glyphs() {
        let (metrics, coverage) = font.rasterize_config(glyph.key);
        let glyph_left = pixel_position(glyph.x);
        let glyph_top = pixel_position(glyph.y + vertical_offset);
        for row in 0..metrics.height {
            for column in 0..metrics.width {
                let alpha = coverage[row * metrics.width + column];
                if alpha == 0 {
                    continue;
                }
                blend_pixel(
                    image,
                    glyph_left + i64::try_from(column).unwrap_or(i64::MAX),
                    glyph_top + i64::try_from(row).unwrap_or(i64::MAX),
                    Rgba([color[0], color[1], color[2], alpha]),
                );
            }
        }
    }
}

fn create_text_layout(
    font: &Font,
    text: &str,
    left: f32,
    maximum_width: f32,
    font_size: f32,
) -> Layout {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x: left,
        y: 0.0,
        max_width: Some(maximum_width),
        horizontal_align: HorizontalAlign::Center,
        line_height: 1.12,
        ..LayoutSettings::default()
    });
    layout.append(&[font], &TextStyle::new(text, font_size, 0));
    layout
}

fn glyph_bounds(layout: &Layout) -> Option<(f32, f32, f32, f32)> {
    layout
        .glyphs()
        .iter()
        .filter(|glyph| glyph.width > 0 && glyph.height > 0)
        .fold(None, |bounds, glyph| {
            let right = glyph.x + glyph_dimension_f32(glyph.width);
            let bottom = glyph.y + glyph_dimension_f32(glyph.height);
            Some(match bounds {
                None => (glyph.x, glyph.y, right, bottom),
                Some((left, top, prior_right, prior_bottom)) => (
                    left.min(glyph.x),
                    top.min(glyph.y),
                    prior_right.max(right),
                    prior_bottom.max(bottom),
                ),
            })
        })
}

fn text_top(canvas_height: f32, text_height: f32, position: TextPosition) -> f32 {
    match position {
        TextPosition::Center => (canvas_height - text_height) / 2.0,
        TextPosition::LowerThird => canvas_height - text_height - canvas_height * 0.09,
        TextPosition::UpperThird => canvas_height * 0.10,
    }
    .max(0.0)
}

fn dimension_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).expect("validated video dimensions fit in u16"))
}

fn glyph_dimension_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("rasterized glyph dimensions fit in u16"))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn bounded_pixel(value: f32, maximum: u32) -> u32 {
    value.round().clamp(0.0, dimension_f32(maximum)) as u32
}

#[allow(clippy::cast_possible_truncation)]
fn pixel_position(value: f32) -> i64 {
    value.floor() as i64
}

fn blend_pixel(image: &mut RgbaImage, x: i64, y: i64, source: Rgba<u8>) {
    let Ok(x) = u32::try_from(x) else {
        return;
    };
    let Ok(y) = u32::try_from(y) else {
        return;
    };
    let Some(destination) = image.get_pixel_mut_checked(x, y) else {
        return;
    };
    let source_alpha = u32::from(source[3]);
    let destination_alpha = u32::from(destination[3]);
    let output_alpha_scaled = source_alpha * 255 + destination_alpha * (255 - source_alpha);
    if output_alpha_scaled == 0 {
        return;
    }
    for channel in 0..3 {
        let source_component = u32::from(source[channel]) * source_alpha * 255;
        let destination_component =
            u32::from(destination[channel]) * destination_alpha * (255 - source_alpha);
        destination[channel] = u8::try_from(
            (source_component + destination_component + output_alpha_scaled / 2)
                / output_alpha_scaled,
        )
        .expect("alpha-composited color channel is bounded to u8");
    }
    destination[3] = u8::try_from((output_alpha_scaled + 127) / 255)
        .expect("alpha-composited opacity is bounded to u8");
}

fn draw_fallback_text(image: &mut RgbaImage, text: &[String], presentation: TextPresentation) {
    let TextPresentation {
        canvas,
        position,
        role,
        color,
        opaque_background,
    } = presentation;
    let scale = match role {
        TextRole::Title => (canvas.height / 130).clamp(2, 12),
        TextRole::Caption => (canvas.height / 270).clamp(2, 8),
    };
    let max_characters = ((canvas.width / (9 * scale)).max(8) - 4) as usize;
    let lines = text
        .iter()
        .flat_map(|line| wrap_text(line, max_characters))
        .collect::<Vec<_>>();
    let line_height = 10 * scale;
    let total_height = line_height * u32::try_from(lines.len()).unwrap_or(1);
    let top = match position {
        TextPosition::Center => canvas.height.saturating_sub(total_height) / 2,
        TextPosition::LowerThird => canvas
            .height
            .saturating_sub(total_height + canvas.height / 10),
        TextPosition::UpperThird => canvas.height / 10,
    };
    let widest = lines
        .iter()
        .map(|line| u32::try_from(line.chars().count()).unwrap_or(0) * 9 * scale)
        .max()
        .unwrap_or(0)
        .min(canvas.width.saturating_sub(2));
    let left = canvas.width.saturating_sub(widest) / 2;
    let margin = 2 * scale;
    fill_rectangle(
        image,
        left.saturating_sub(margin),
        top.saturating_sub(margin),
        (widest + 2 * margin).min(canvas.width),
        (total_height + 2 * margin).min(canvas.height),
        if opaque_background {
            Rgba([4, 8, 12, 255])
        } else {
            Rgba([0, 0, 0, 176])
        },
    );
    for (line_index, line) in lines.iter().enumerate() {
        let line_width = u32::try_from(line.chars().count()).unwrap_or(0) * 9 * scale;
        let x = canvas.width.saturating_sub(line_width) / 2;
        let y = top + u32::try_from(line_index).unwrap_or(0) * line_height;
        draw_bitmap_text(
            image,
            line,
            x,
            y,
            scale,
            Rgba([color[0], color[1], color[2], 255]),
        );
    }
}

fn wrap_text(text: &str, max_characters: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > max_characters {
            lines.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.extend(word.chars().take(max_characters));
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn fill_rectangle(
    image: &mut RgbaImage,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    color: Rgba<u8>,
) {
    for y in top..top.saturating_add(height).min(image.height()) {
        for x in left..left.saturating_add(width).min(image.width()) {
            image.put_pixel(x, y, color);
        }
    }
}

fn draw_bitmap_text(
    image: &mut RgbaImage,
    text: &str,
    left: u32,
    top: u32,
    scale: u32,
    color: Rgba<u8>,
) {
    for (character_index, character) in text.chars().enumerate() {
        let glyph = BASIC_FONTS
            .get(character)
            .or_else(|| BASIC_FONTS.get('?'))
            .unwrap_or([0; 8]);
        let glyph_left = left + u32::try_from(character_index).unwrap_or(0) * 9 * scale;
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0_u32..8 {
                if bits & (1 << column) == 0 {
                    continue;
                }
                fill_rectangle(
                    image,
                    glyph_left + column * scale,
                    top + u32::try_from(row).unwrap_or(0) * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
    }
}

fn parse_hex_color(value: &str) -> [u8; 3] {
    let value = value.trim_start_matches('#');
    let channel = |start| u8::from_str_radix(&value[start..start + 2], 16).unwrap_or(255);
    [channel(0), channel(2), channel(4)]
}

fn publish_without_overwrite(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::hard_link(source, destination)
}

#[derive(Debug, Deserialize)]
struct ProbeDocument {
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_wrap_without_executable_filter_text() {
        assert_eq!(
            wrap_text("record once and publish thoughtfully", 12),
            vec!["record once", "and publish", "thoughtfully"]
        );
    }

    #[test]
    fn accent_colors_are_parsed_in_rust() {
        assert_eq!(parse_hex_color("#66D9FF"), [102, 217, 255]);
    }

    #[test]
    fn typed_motion_filters_never_include_project_expressions() {
        let segment: TimelineSegment = serde_json::from_value(serde_json::json!({
            "segmentId": "pan",
            "kind": "titleCard",
            "startMs": 0,
            "durationMs": 1000,
            "text": {
                "heading": "Safe title",
                "position": "center",
                "accentColor": "#66D9FF"
            },
            "motion": "panLeft",
            "crop": "fill",
            "audioGainDb": 0,
            "transition": {"kind": "cut", "durationMs": 0}
        }))
        .unwrap();
        let filter = motion_filter(
            &segment,
            Canvas {
                width: 1280,
                height: 720,
            },
        );
        assert!(filter.contains("crop=1280:720"));
        assert!(!filter.contains("Safe title"));
    }
}
