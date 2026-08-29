use act_interfaces::creator_media::{
    CreatorRenderReceipt, OutputReceipt, PublicationReceipt, ReceiptReviewState, RenderStatus,
    ReviewState, SourceAsset, SourceLineage, Toolchain,
};
use chrono::{SecondsFormat, Utc};

use super::project::ValidatedProject;

pub(super) fn successful_receipt(
    project: &ValidatedProject,
    ffmpeg_version: String,
    outputs: Vec<OutputReceipt>,
) -> CreatorRenderReceipt {
    let now = Utc::now();
    let review_state = match project.project.review.state {
        ReviewState::Approved => ReceiptReviewState::Approved,
        ReviewState::Rejected => ReceiptReviewState::Rejected,
        ReviewState::Draft | ReviewState::NeedsReview => ReceiptReviewState::NeedsReview,
    };
    let private_upload_eligible = review_state == ReceiptReviewState::Approved
        && project.project.publication.rights_confirmed
        && project
            .project
            .assets
            .iter()
            .all(|asset| asset.rights.approved);

    CreatorRenderReceipt {
        schema_version: "1.0".into(),
        render_id: format!(
            "render-{}-{}",
            now.timestamp(),
            now.timestamp_subsec_millis()
        ),
        project_id: project.project.project_id.clone(),
        status: RenderStatus::Succeeded,
        source: SourceLineage {
            project_sha256: project.project_sha256.clone(),
            assets: project
                .project
                .assets
                .iter()
                .map(|asset| SourceAsset {
                    asset_id: asset.asset_id.clone(),
                    sha256: project.assets[&asset.asset_id].sha256.clone(),
                })
                .collect(),
        },
        outputs,
        toolchain: Toolchain {
            renderer: "act-desktop-app.rs".into(),
            renderer_version: env!("CARGO_PKG_VERSION").into(),
            ffmpeg_version,
        },
        review_state,
        publication: PublicationReceipt {
            provider: project.project.publication.provider,
            channel_handle: project.project.publication.channel_handle.clone(),
            channel_id: project.project.publication.channel_id.clone(),
            privacy_status: project.project.publication.privacy_status,
            private_upload_eligible,
            public_eligible: false,
        },
        failure: None,
        created_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_ids_fit_the_language_neutral_identifier_contract() {
        let now = Utc::now();
        let render_id = format!(
            "render-{}-{}",
            now.timestamp(),
            now.timestamp_subsec_millis()
        );
        assert!(
            render_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        );
    }
}
