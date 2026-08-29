//! Native, private-first creator rendering.
//!
//! The renderer consumes the versioned `act-interfaces` control contract. It
//! never accepts shell commands or filter expressions from a project file;
//! `FFmpeg` arguments are generated from bounded typed values in this module.

mod ffmpeg;
mod project;
mod receipt;

use std::path::{Path, PathBuf};
use std::time::Duration;

use act_interfaces::creator_media::{CreatorRenderReceipt, RenderFailureCode};
use thiserror::Error;

use self::ffmpeg::FfmpegEngine;
use self::project::ValidatedProject;

pub use self::project::{EXPECTED_CHANNEL_ID, EXPECTED_CREATOR_HANDLE};

/// Stable runtime identifier surfaced by the Qt studio and render receipts.
pub const RENDERER_NAME: &str = "act-creator-renderer";

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Local renderer configuration. Provider credentials are deliberately absent.
#[derive(Clone, Debug)]
pub struct RendererConfig {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub command_timeout: Duration,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }
}

/// A native renderer whose subprocess boundary is an argument vector, never a
/// shell. One instance is safe to reuse for sequential projects.
#[derive(Clone, Debug)]
pub struct NativeRenderer {
    engine: FfmpegEngine,
}

impl NativeRenderer {
    #[must_use]
    pub fn new(config: RendererConfig) -> Self {
        Self {
            engine: FfmpegEngine::new(config),
        }
    }

    /// Validates a creator project, renders every declared output, and returns
    /// a checksum/probe-backed private-publication receipt.
    ///
    /// Existing output files are never overwritten. The project and every
    /// source asset must resolve inside `project_root`.
    ///
    /// # Errors
    ///
    /// Returns a typed failure when validation, rights, rendering, or probing
    /// fails. Error messages contain asset/output identifiers rather than
    /// absolute local paths.
    pub fn render(
        &self,
        project_path: &Path,
        project_root: &Path,
    ) -> Result<CreatorRenderReceipt, RenderError> {
        let project = ValidatedProject::load(project_path, project_root)?;
        self.engine.render(&project)
    }
}

/// Bounded failure taxonomy shared with the language-neutral receipt contract.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("invalid creator project: {0}")]
    InvalidProject(String),
    #[error("asset not found: {0}")]
    AssetNotFound(String),
    #[error("asset digest does not match the project contract: {0}")]
    AssetDigestMismatch(String),
    #[error("asset rights are not approved: {0}")]
    RightsNotApproved(String),
    #[error("effect is not supported by the native renderer: {0}")]
    UnsupportedEffect(String),
    #[error("invalid output: {0}")]
    InvalidOutput(String),
    #[error("FFmpeg or FFprobe is unavailable: {0}")]
    RendererUnavailable(String),
    #[error("native render failed: {0}")]
    RenderFailed(String),
    #[error("rendered output probe failed: {0}")]
    ProbeFailed(String),
}

impl RenderError {
    #[must_use]
    pub const fn code(&self) -> RenderFailureCode {
        match self {
            Self::InvalidProject(_) => RenderFailureCode::InvalidProject,
            Self::AssetNotFound(_) => RenderFailureCode::AssetNotFound,
            Self::AssetDigestMismatch(_) => RenderFailureCode::AssetDigestMismatch,
            Self::RightsNotApproved(_) => RenderFailureCode::RightsNotApproved,
            Self::UnsupportedEffect(_) => RenderFailureCode::UnsupportedEffect,
            Self::InvalidOutput(_) => RenderFailureCode::InvalidOutput,
            Self::RendererUnavailable(_) => RenderFailureCode::RendererUnavailable,
            Self::RenderFailed(_) => RenderFailureCode::RenderFailed,
            Self::ProbeFailed(_) => RenderFailureCode::ProbeFailed,
        }
    }
}
