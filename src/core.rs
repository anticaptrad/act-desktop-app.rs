use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

pub const EXPECTED_CREATOR_HANDLE: &str = "@anticaptrad";
pub const CONTROL_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionPhase {
    Idle,
    Resolving,
    Signaling,
    Negotiating,
    Live,
    Recovering,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalingEndpoint(Url);

impl SignalingEndpoint {
    /// Validates and normalizes a signaling endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL is malformed, embeds credentials, contains
    /// a fragment, or uses cleartext transport outside loopback development.
    pub fn parse(value: &str) -> Result<Self, EndpointError> {
        let url = Url::parse(value).map_err(|_| EndpointError::InvalidUrl)?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(EndpointError::EmbeddedCredentials);
        }
        if url.fragment().is_some() {
            return Err(EndpointError::FragmentNotAllowed);
        }

        let secure = matches!(url.scheme(), "https" | "wss");
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "::1" | "localhost"));
        let local_transport = loopback && matches!(url.scheme(), "http" | "ws");
        if !secure && !local_transport {
            return Err(EndpointError::InsecureTransport);
        }

        Ok(Self(url))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EndpointError {
    #[error("signaling endpoint is not a valid URL")]
    InvalidUrl,
    #[error("signaling endpoint must use HTTPS or WSS outside loopback development")]
    InsecureTransport,
    #[error("credentials must never be embedded in a signaling URL")]
    EmbeddedCredentials,
    #[error("URL fragments are not accepted by the signaling contract")]
    FragmentNotAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaBudget {
    pub max_video_frames: usize,
    pub max_audio_packets: usize,
    pub max_control_messages: usize,
}

impl Default for MediaBudget {
    fn default() -> Self {
        Self {
            max_video_frames: 3,
            max_audio_packets: 64,
            max_control_messages: CONTROL_QUEUE_CAPACITY,
        }
    }
}

impl MediaBudget {
    #[must_use]
    pub fn validates_realtime_bounds(self) -> bool {
        (1..=8).contains(&self.max_video_frames)
            && (8..=256).contains(&self.max_audio_packets)
            && (32..=1_024).contains(&self.max_control_messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_secure_and_loopback_signaling() {
        assert!(SignalingEndpoint::parse("wss://signal.anticaptrad.example/v1").is_ok());
        assert!(SignalingEndpoint::parse("http://127.0.0.1:9000/session").is_ok());
    }

    #[test]
    fn rejects_remote_cleartext_and_url_credentials() {
        assert_eq!(
            SignalingEndpoint::parse("ws://signal.anticaptrad.example/v1"),
            Err(EndpointError::InsecureTransport)
        );
        assert_eq!(
            SignalingEndpoint::parse("wss://user:secret@signal.example/v1"),
            Err(EndpointError::EmbeddedCredentials)
        );
    }

    #[test]
    fn realtime_budget_is_bounded() {
        assert!(MediaBudget::default().validates_realtime_bounds());
        assert!(
            !MediaBudget {
                max_video_frames: 256,
                ..MediaBudget::default()
            }
            .validates_realtime_bounds()
        );
    }
}
