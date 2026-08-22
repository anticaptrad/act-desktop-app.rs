use std::net::SocketAddr;
use std::time::Duration;

use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::runtime::{Builder, Runtime};
use tokio::time::timeout;

const UDP_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct RuntimeSupervisor {
    runtime: Runtime,
}

impl RuntimeSupervisor {
    /// Creates the bounded background runtime used by media and signaling tasks.
    ///
    /// # Errors
    ///
    /// Returns an error when Tokio cannot allocate its worker runtime.
    pub fn new() -> Result<Self, RuntimeError> {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("act-media")
            .build()
            .map_err(RuntimeError::Create)?;

        Ok(Self { runtime })
    }

    /// Exchanges a datagram between two loopback sockets as a transport diagnostic.
    ///
    /// # Errors
    ///
    /// Returns an error when sockets cannot bind, the exchange times out, or the
    /// received datagram does not match the probe payload.
    pub fn probe_udp_loopback(&self) -> Result<SocketAddr, RuntimeError> {
        self.runtime.block_on(async {
            timeout(UDP_PROBE_TIMEOUT, async {
                let receiver = UdpSocket::bind("127.0.0.1:0")
                    .await
                    .map_err(RuntimeError::Udp)?;
                let sender = UdpSocket::bind("127.0.0.1:0")
                    .await
                    .map_err(RuntimeError::Udp)?;
                let destination = receiver.local_addr().map_err(RuntimeError::Udp)?;
                sender
                    .send_to(b"act-transport-probe", destination)
                    .await
                    .map_err(RuntimeError::Udp)?;

                let mut packet = [0_u8; 64];
                let (packet_length, peer) = receiver
                    .recv_from(&mut packet)
                    .await
                    .map_err(RuntimeError::Udp)?;
                if &packet[..packet_length] != b"act-transport-probe" {
                    return Err(RuntimeError::UnexpectedPayload);
                }

                Ok(peer)
            })
            .await
            .map_err(|_| RuntimeError::Timeout)?
        })
    }

    #[must_use]
    pub const fn stack_summary() -> &'static str {
        "Qt Quick/QML + CXX-Qt + Tokio + webrtc-rs"
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("could not create the Tokio media runtime: {0}")]
    Create(std::io::Error),
    #[error("UDP transport probe failed: {0}")]
    Udp(std::io::Error),
    #[error("UDP transport probe timed out")]
    Timeout,
    #[error("UDP transport probe returned an unexpected payload")]
    UnexpectedPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_transport_is_available() {
        let runtime = RuntimeSupervisor::new().expect("Tokio runtime should start");
        let peer = runtime
            .probe_udp_loopback()
            .expect("loopback UDP should exchange a packet");
        assert!(peer.ip().is_loopback());
    }
}
