use crate::pinned_cert_verifier::PinnedCertVerifier;
use anyhow::{Context, Result, anyhow};
use rustls::{ClientConfig, pki_types::ServerName};
use std::{sync::Arc, time::Duration};
use tokio::{io::BufReader, net::TcpStream};
use tokio_rustls::TlsConnector;

/// The reader half of a client connection.
pub type ClientReader = tokio::io::BufReader<
    tokio::io::ReadHalf<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
>;

/// The writer half of a client connection.
pub type ClientWriter =
    tokio::io::WriteHalf<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>;

/// Connects to the server at `addr` with TLS using a pinned cert verifier from the file at `path`,
/// timing out after `timeout`. Immediately splits into reader and writer halves.
///
/// # Errors
///
/// Returns `Err` if the file reading or TLS connection process fails or times out.
pub async fn connect(
    path: &str,
    addr: &str,
    timeout: Duration,
) -> Result<(ClientReader, ClientWriter)> {
    // Create a TLS client that validates against the pinned certificate
    let connector = TlsConnector::from(Arc::new(
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedCertVerifier::from_file(path)?))
            .with_no_client_auth(),
    ));

    // Connect to the server with a timeout
    let socket = tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .context("Timeout connecting to server")??;

    let host = addr
        .split_once(':')
        .with_context(|| format!("Failed to split addr {addr} on ':'"))?
        .0
        .to_string();

    let server_name = ServerName::try_from(host).map_err(|e| anyhow!("Invalid DNS name: {e}"))?;

    // Perform TLS handshake with a timeout
    let tls_stream = tokio::time::timeout(timeout, connector.connect(server_name, socket))
        .await
        .context("Timeout during TLS handshake")??;

    let (reader, writer) = tokio::io::split(tls_stream);

    Ok((BufReader::new(reader), writer))
}
