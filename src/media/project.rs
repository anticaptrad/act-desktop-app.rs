use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use act_interfaces::creator_media::{
    AspectRatio, AssetKind, CreatorAsset, CreatorMediaProject, Crop, OutputKind,
    PublicationProvider, SegmentKind, SourceType, TimelineSegment, TransitionKind,
};
use sha2::{Digest, Sha256};

use super::RenderError;

pub const EXPECTED_CREATOR_HANDLE: &str = "@anticaptrad";
pub const EXPECTED_CHANNEL_ID: &str = "UC-Gloecwemo_Mh-VAjnUipg";

const MAX_PROJECT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TIMELINE_MS: u64 = 4 * 60 * 60 * 1_000;
const MAX_TRANSITION_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug)]
pub(super) struct Canvas {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedAsset {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug)]
pub(super) struct ValidatedProject {
    pub project: CreatorMediaProject,
    pub root: PathBuf,
    pub project_sha256: String,
    pub assets: BTreeMap<String, ResolvedAsset>,
    pub outputs: BTreeMap<String, PathBuf>,
    pub canvas: Canvas,
    pub timeline_duration_ms: u64,
}

impl ValidatedProject {
    pub fn load(project_path: &Path, project_root: &Path) -> Result<Self, RenderError> {
        let root = project_root.canonicalize().map_err(|error| {
            RenderError::InvalidProject(format!("project root is unavailable: {error}"))
        })?;
        if !root.is_dir() {
            return Err(RenderError::InvalidProject(
                "project root must be a directory".into(),
            ));
        }

        let canonical_project = project_path.canonicalize().map_err(|error| {
            RenderError::InvalidProject(format!("project file is unavailable: {error}"))
        })?;
        if !canonical_project.starts_with(&root) || !canonical_project.is_file() {
            return Err(RenderError::InvalidProject(
                "project file must be a regular file inside the project root".into(),
            ));
        }

        let metadata = canonical_project.metadata().map_err(|error| {
            RenderError::InvalidProject(format!("project metadata is unavailable: {error}"))
        })?;
        if metadata.len() > MAX_PROJECT_BYTES {
            return Err(RenderError::InvalidProject(
                "project file exceeds the 4 MiB control-plane limit".into(),
            ));
        }
        let raw = fs::read(&canonical_project).map_err(|error| {
            RenderError::InvalidProject(format!("project file could not be read: {error}"))
        })?;
        let project_sha256 = sha256_bytes(&raw);
        let project: CreatorMediaProject = serde_json::from_slice(&raw).map_err(|error| {
            RenderError::InvalidProject(format!("project JSON does not match v1: {error}"))
        })?;

        validate_identity(&project)?;
        validate_identifier(&project.project_id, "projectId")?;
        if project.title.trim().is_empty() || project.title.chars().count() > 200 {
            return Err(RenderError::InvalidProject(
                "title must contain 1 to 200 characters".into(),
            ));
        }
        if project.assets.is_empty() || project.assets.len() > 100 {
            return Err(RenderError::InvalidProject(
                "assets must contain 1 to 100 entries".into(),
            ));
        }
        if project.timeline.is_empty() || project.timeline.len() > 500 {
            return Err(RenderError::InvalidProject(
                "timeline must contain 1 to 500 segments".into(),
            ));
        }
        if project.outputs.is_empty() || project.outputs.len() > 20 {
            return Err(RenderError::InvalidProject(
                "outputs must contain 1 to 20 entries".into(),
            ));
        }

        let assets = validate_assets(&project, &root)?;
        let timeline_duration_ms = validate_timeline(&project, &assets)?;
        validate_captions_and_audio(&project, timeline_duration_ms, &assets)?;
        let outputs = validate_outputs(&project, &root, timeline_duration_ms)?;
        let canvas_output = project
            .outputs
            .iter()
            .find(|output| output.kind == OutputKind::Master)
            .or_else(|| {
                project
                    .outputs
                    .iter()
                    .max_by_key(|output| u64::from(output.width) * u64::from(output.height))
            })
            .expect("non-empty outputs were validated");
        let canvas = Canvas {
            width: canvas_output.width,
            height: canvas_output.height,
        };

        Ok(Self {
            project,
            root,
            project_sha256,
            assets,
            outputs,
            canvas,
            timeline_duration_ms,
        })
    }
}

fn validate_identity(project: &CreatorMediaProject) -> Result<(), RenderError> {
    if project.schema_version != "1.0" {
        return Err(RenderError::InvalidProject(
            "schemaVersion must be 1.0".into(),
        ));
    }
    if project.creator_handle != EXPECTED_CREATOR_HANDLE
        || project.publication.channel_handle != EXPECTED_CREATOR_HANDLE
        || project.publication.channel_id != EXPECTED_CHANNEL_ID
        || project.publication.provider != PublicationProvider::Youtube
    {
        return Err(RenderError::InvalidProject(
            "creator and YouTube channel identity must match AntiCapTrad".into(),
        ));
    }
    if project.publication.allow_public {
        return Err(RenderError::InvalidProject(
            "allowPublic must remain false; public publishing is outside the renderer".into(),
        ));
    }
    if !project.publication.rights_confirmed {
        return Err(RenderError::RightsNotApproved(
            "publication.rightsConfirmed".into(),
        ));
    }
    Ok(())
}

fn validate_assets(
    project: &CreatorMediaProject,
    root: &Path,
) -> Result<BTreeMap<String, ResolvedAsset>, RenderError> {
    let mut assets = BTreeMap::new();
    for asset in &project.assets {
        validate_identifier(&asset.asset_id, "assetId")?;
        if assets.contains_key(&asset.asset_id) {
            return Err(RenderError::InvalidProject(format!(
                "duplicate assetId {}",
                asset.asset_id
            )));
        }
        validate_relative_path(&asset.relative_path, "asset relativePath")?;
        if !asset.rights.approved || asset.rights.owner.trim().is_empty() {
            return Err(RenderError::RightsNotApproved(asset.asset_id.clone()));
        }
        if matches!(
            asset.rights.source_type,
            SourceType::LicensedStock | SourceType::SoundCue
        ) && [
            asset.rights.source_url.as_deref(),
            asset.rights.license_id.as_deref(),
            asset.rights.license_name.as_deref(),
            asset.rights.attribution.as_deref(),
        ]
        .iter()
        .any(|value| value.is_none_or(|text| text.trim().is_empty()))
        {
            return Err(RenderError::RightsNotApproved(format!(
                "{} is missing license provenance",
                asset.asset_id
            )));
        }
        if !is_sha256(&asset.sha256) {
            return Err(RenderError::InvalidProject(format!(
                "asset {} has an invalid SHA-256 digest",
                asset.asset_id
            )));
        }

        let candidate = root.join(&asset.relative_path);
        let path = candidate
            .canonicalize()
            .map_err(|_| RenderError::AssetNotFound(asset.asset_id.clone()))?;
        if !path.starts_with(root) || !path.is_file() {
            return Err(RenderError::AssetNotFound(asset.asset_id.clone()));
        }
        let actual_sha256 = sha256_file(&path).map_err(|_| {
            RenderError::AssetNotFound(format!("{} could not be read", asset.asset_id))
        })?;
        if actual_sha256 != asset.sha256 {
            return Err(RenderError::AssetDigestMismatch(asset.asset_id.clone()));
        }

        assets.insert(
            asset.asset_id.clone(),
            ResolvedAsset {
                path,
                sha256: actual_sha256,
            },
        );
    }
    Ok(assets)
}

fn validate_timeline(
    project: &CreatorMediaProject,
    assets: &BTreeMap<String, ResolvedAsset>,
) -> Result<u64, RenderError> {
    let assets_by_id = project
        .assets
        .iter()
        .map(|asset| (asset.asset_id.as_str(), asset))
        .collect::<BTreeMap<_, _>>();
    let mut segment_ids = BTreeSet::new();
    let mut cursor = 0_u64;
    for segment in &project.timeline {
        cursor = validate_segment_basics(segment, cursor, &mut segment_ids)?;
        validate_segment_content(segment, &assets_by_id)?;
        validate_segment_asset(segment, assets, &assets_by_id)?;
    }
    Ok(cursor)
}

fn validate_segment_basics<'a>(
    segment: &'a TimelineSegment,
    cursor: u64,
    segment_ids: &mut BTreeSet<&'a str>,
) -> Result<u64, RenderError> {
    validate_identifier(&segment.segment_id, "segmentId")?;
    if !segment_ids.insert(segment.segment_id.as_str()) {
        return Err(RenderError::InvalidProject(format!(
            "duplicate segmentId {}",
            segment.segment_id
        )));
    }
    if segment.start_ms != cursor || segment.duration_ms == 0 {
        return Err(RenderError::InvalidProject(format!(
            "segment {} must start at {cursor}ms and have positive duration",
            segment.segment_id
        )));
    }
    let next_cursor = cursor
        .checked_add(segment.duration_ms)
        .ok_or_else(|| RenderError::InvalidProject("timeline duration overflowed".into()))?;
    if next_cursor > MAX_TIMELINE_MS {
        return Err(RenderError::InvalidProject(
            "timeline exceeds the four-hour limit".into(),
        ));
    }
    if !(-60.0..=12.0).contains(&segment.audio_gain_db) {
        return Err(RenderError::InvalidProject(format!(
            "segment {} audioGainDb is outside -60..12",
            segment.segment_id
        )));
    }
    if segment.crop == Crop::FaceAware {
        return Err(RenderError::UnsupportedEffect(format!(
            "segment {} requests faceAware crop before a detector has verified a subject",
            segment.segment_id
        )));
    }
    validate_transition(segment)?;
    validate_segment_text(segment)?;
    Ok(next_cursor)
}

fn validate_segment_text(segment: &TimelineSegment) -> Result<(), RenderError> {
    let Some(text) = &segment.text else {
        return Ok(());
    };
    if text.heading.trim().is_empty() || text.heading.chars().count() > 160 {
        return Err(RenderError::InvalidProject(format!(
            "segment {} heading must contain 1 to 160 characters",
            segment.segment_id
        )));
    }
    if text
        .subheading
        .as_ref()
        .is_some_and(|value| value.chars().count() > 240)
        || !is_hex_color(&text.accent_color)
    {
        return Err(RenderError::InvalidProject(format!(
            "segment {} text styling is outside renderer bounds",
            segment.segment_id
        )));
    }
    Ok(())
}

fn validate_segment_content(
    segment: &TimelineSegment,
    assets_by_id: &BTreeMap<&str, &CreatorAsset>,
) -> Result<(), RenderError> {
    match segment.kind {
        SegmentKind::Source => {
            let asset_id = segment.asset_id.as_deref().ok_or_else(|| {
                RenderError::InvalidProject(format!(
                    "source segment {} requires assetId",
                    segment.segment_id
                ))
            })?;
            if !assets_by_id.contains_key(asset_id) {
                return Err(RenderError::AssetNotFound(format!(
                    "segment {}",
                    segment.segment_id
                )));
            }
        }
        SegmentKind::TitleCard if segment.text.is_none() => {
            return Err(RenderError::InvalidProject(format!(
                "title card {} requires text",
                segment.segment_id
            )));
        }
        SegmentKind::Interstitial if segment.text.is_none() && segment.asset_id.is_none() => {
            return Err(RenderError::InvalidProject(format!(
                "interstitial {} requires text or a visual asset",
                segment.segment_id
            )));
        }
        SegmentKind::TitleCard | SegmentKind::Interstitial => {}
    }
    Ok(())
}

fn validate_segment_asset(
    segment: &TimelineSegment,
    assets: &BTreeMap<String, ResolvedAsset>,
    assets_by_id: &BTreeMap<&str, &CreatorAsset>,
) -> Result<(), RenderError> {
    let Some(asset_id) = &segment.asset_id else {
        return Ok(());
    };
    if !assets.contains_key(asset_id) {
        return Err(RenderError::AssetNotFound(asset_id.clone()));
    }
    let asset = assets_by_id[asset_id.as_str()];
    if !matches!(
        asset.kind,
        AssetKind::Camera | AssetKind::StockVideo | AssetKind::StockImage
    ) {
        return Err(RenderError::UnsupportedEffect(format!(
            "segment {} uses a non-visual asset",
            segment.segment_id
        )));
    }
    let Some(asset_duration_ms) = asset.duration_ms else {
        return Ok(());
    };
    let source_end = segment
        .source_in_ms
        .unwrap_or(0)
        .checked_add(segment.duration_ms)
        .ok_or_else(|| {
            RenderError::InvalidProject(format!(
                "segment {} source range overflowed",
                segment.segment_id
            ))
        })?;
    if asset.kind != AssetKind::StockImage && source_end > asset_duration_ms {
        return Err(RenderError::InvalidProject(format!(
            "segment {} exceeds source asset duration",
            segment.segment_id
        )));
    }
    Ok(())
}

fn validate_transition(segment: &TimelineSegment) -> Result<(), RenderError> {
    match segment.transition.kind {
        TransitionKind::Cut if segment.transition.duration_ms != 0 => {
            Err(RenderError::InvalidProject(format!(
                "cut transition on {} must have zero duration",
                segment.segment_id
            )))
        }
        TransitionKind::Fade | TransitionKind::Dissolve
            if segment.transition.duration_ms == 0
                || segment.transition.duration_ms > MAX_TRANSITION_MS
                || segment.transition.duration_ms * 2 > segment.duration_ms =>
        {
            Err(RenderError::InvalidProject(format!(
                "transition on {} must be 1..2000ms and at most half the segment",
                segment.segment_id
            )))
        }
        _ => Ok(()),
    }
}

fn validate_captions_and_audio(
    project: &CreatorMediaProject,
    timeline_duration_ms: u64,
    assets: &BTreeMap<String, ResolvedAsset>,
) -> Result<(), RenderError> {
    if !(-24.0..=-9.0).contains(&project.audio.target_lufs)
        || !(-6.0..=0.0).contains(&project.audio.true_peak_db)
    {
        return Err(RenderError::InvalidProject(
            "audio loudness target is outside broadcast-safe bounds".into(),
        ));
    }
    for caption in &project.captions {
        if caption.start_ms >= caption.end_ms
            || caption.end_ms > timeline_duration_ms
            || caption.text.trim().is_empty()
            || caption.text.chars().count() > 500
        {
            return Err(RenderError::InvalidProject(
                "caption timing or text is invalid".into(),
            ));
        }
    }
    let assets_by_id = project
        .assets
        .iter()
        .map(|asset| (asset.asset_id.as_str(), asset))
        .collect::<BTreeMap<_, _>>();
    for cue in &project.audio.cues {
        if !assets.contains_key(&cue.asset_id)
            || !assets_by_id
                .get(cue.asset_id.as_str())
                .is_some_and(|asset| asset.kind == AssetKind::SoundCue)
            || cue.start_ms >= timeline_duration_ms
            || !(-60.0..=6.0).contains(&cue.gain_db)
        {
            return Err(RenderError::InvalidProject(format!(
                "audio cue {} is invalid",
                cue.asset_id
            )));
        }
    }
    Ok(())
}

fn validate_outputs(
    project: &CreatorMediaProject,
    root: &Path,
    timeline_duration_ms: u64,
) -> Result<BTreeMap<String, PathBuf>, RenderError> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut outputs = BTreeMap::new();
    for output in &project.outputs {
        validate_identifier(&output.output_id, "outputId")?;
        validate_relative_path(&output.relative_path, "output relativePath")?;
        if !ids.insert(output.output_id.as_str()) || !paths.insert(output.relative_path.as_str()) {
            return Err(RenderError::InvalidOutput(format!(
                "duplicate output {}",
                output.output_id
            )));
        }
        if output.width < 320
            || output.height < 320
            || output.width > 7_680
            || output.height > 4_320
            || output.width % 2 != 0
            || output.height % 2 != 0
            || !aspect_matches(output.aspect_ratio, output.width, output.height)
            || !(0.0..=20.0).contains(&output.safe_area_percent)
        {
            return Err(RenderError::InvalidOutput(format!(
                "{} has invalid dimensions, aspect ratio, or safe area",
                output.output_id
            )));
        }
        match (output.kind, &output.source_window) {
            (OutputKind::Clip, Some(window))
                if !(30_000..=50_000).contains(&output.duration_ms)
                    || !source_window_is_valid(
                        window.start_ms,
                        window.end_ms,
                        output.duration_ms,
                        timeline_duration_ms,
                    ) =>
            {
                return Err(RenderError::InvalidOutput(format!(
                    "clip {} must be a 30-50 second in-bounds source window",
                    output.output_id
                )));
            }
            (OutputKind::Clip, None) => {
                return Err(RenderError::InvalidOutput(format!(
                    "clip {} requires sourceWindow",
                    output.output_id
                )));
            }
            (_, Some(window))
                if !source_window_is_valid(
                    window.start_ms,
                    window.end_ms,
                    output.duration_ms,
                    timeline_duration_ms,
                ) =>
            {
                return Err(RenderError::InvalidOutput(format!(
                    "output {} has an invalid sourceWindow",
                    output.output_id
                )));
            }
            (_, None) if output.duration_ms != timeline_duration_ms => {
                return Err(RenderError::InvalidOutput(format!(
                    "output {} duration must equal the timeline",
                    output.output_id
                )));
            }
            _ => {}
        }

        let destination = root.join(&output.relative_path);
        if destination.exists() {
            return Err(RenderError::InvalidOutput(format!(
                "{} already exists; outputs are never overwritten",
                output.output_id
            )));
        }
        validate_existing_ancestors(root, &destination, &output.output_id)?;
        outputs.insert(output.output_id.clone(), destination);
    }
    Ok(outputs)
}

fn source_window_is_valid(start_ms: u64, end_ms: u64, duration_ms: u64, limit_ms: u64) -> bool {
    end_ms <= limit_ms && end_ms.checked_sub(start_ms) == Some(duration_ms)
}

fn validate_existing_ancestors(
    root: &Path,
    destination: &Path,
    output_id: &str,
) -> Result<(), RenderError> {
    let relative = destination
        .strip_prefix(root)
        .map_err(|_| RenderError::InvalidOutput(format!("{output_id} escapes the project root")))?;
    let mut cursor = root.to_path_buf();
    for component in relative
        .components()
        .take(relative.components().count() - 1)
    {
        cursor.push(component);
        if cursor.exists() {
            let canonical = cursor.canonicalize().map_err(|_| {
                RenderError::InvalidOutput(format!("{output_id} parent is unavailable"))
            })?;
            if !canonical.starts_with(root) || !canonical.is_dir() {
                return Err(RenderError::InvalidOutput(format!(
                    "{output_id} parent escapes the project root"
                )));
            }
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<(), RenderError> {
    let valid = !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(RenderError::InvalidProject(format!(
            "{field} is not a valid identifier"
        )))
    }
}

fn validate_relative_path(value: &str, field: &str) -> Result<(), RenderError> {
    let path = Path::new(value);
    let valid = !value.is_empty()
        && value.len() <= 500
        && !path.is_absolute()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if valid {
        Ok(())
    } else {
        Err(RenderError::InvalidProject(format!(
            "{field} must be a normalized relative path"
        )))
    }
}

fn aspect_matches(aspect: AspectRatio, width: u32, height: u32) -> bool {
    match aspect {
        AspectRatio::Landscape => u64::from(width) * 9 == u64::from(height) * 16,
        AspectRatio::Portrait => u64::from(width) * 16 == u64::from(height) * 9,
        AspectRatio::Square => width == height,
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(super) fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format_digest(&digest.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format_digest(&Sha256::digest(bytes))
}

fn format_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_reject_traversal_and_absolute_inputs() {
        assert!(validate_relative_path("assets/camera.mp4", "path").is_ok());
        assert!(validate_relative_path("../camera.mp4", "path").is_err());
        assert!(validate_relative_path("assets/../camera.mp4", "path").is_err());
        assert!(validate_relative_path("/tmp/camera.mp4", "path").is_err());
        assert!(validate_relative_path("assets/camera concept.mp4", "path").is_err());
    }

    #[test]
    fn output_aspects_must_be_exact() {
        assert!(aspect_matches(AspectRatio::Landscape, 1280, 720));
        assert!(aspect_matches(AspectRatio::Portrait, 720, 1280));
        assert!(aspect_matches(AspectRatio::Square, 720, 720));
        assert!(!aspect_matches(AspectRatio::Landscape, 1278, 720));
    }

    #[test]
    fn digest_format_is_lowercase_sha256() {
        assert_eq!(
            sha256_bytes(b"anticaptrad"),
            "9f7fcd89c5fc8d182c0a1b62d65b4492b78bf0c2bebe6bd2fcb91cf66a5737dd"
        );
    }

    #[test]
    fn identifier_is_bounded_and_shell_neutral() {
        assert!(validate_identifier("clip-vertical-01", "id").is_ok());
        assert!(validate_identifier("clip;touch-owned", "id").is_err());
        assert!(validate_identifier("-starts-with-dash", "id").is_err());
    }

    #[test]
    fn approved_review_is_not_required_for_local_preview() {
        assert_ne!(
            act_interfaces::creator_media::ReviewState::Draft,
            act_interfaces::creator_media::ReviewState::Approved
        );
    }
}
