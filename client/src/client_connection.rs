use crate::pinned_cert_verifier::PinnedCertVerifier;
use anyhow::{Context, Result, anyhow};
use rustls::{ClientConfig, pki_types::ServerName};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::TcpStream,
};
use tokio_rustls::{TlsConnector, client::TlsStream};

/// Manages a client's TLS connection to the server.
pub struct ClientConnection {
    reader: BufReader<ReadHalf<TlsStream<TcpStream>>>,
    writer: WriteHalf<TlsStream<TcpStream>>,
}

impl ClientConnection {
    /// Connects to the server at `addr` with TLS using the pinned cert verifier, timing out after
    /// `timeout`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the connection process fails or times out.
    pub async fn connect(addr: &str, timeout: Duration) -> Result<Self> {
        // Create a TLS client that validates against the pinned certificate
        let connector = TlsConnector::from(Arc::new(
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(PinnedCertVerifier::from_file()?))
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

        let server_name =
            ServerName::try_from(host).map_err(|e| anyhow!("Invalid DNS name: {e}"))?;

        // Perform TLS handshake with a timeout
        let tls_stream = tokio::time::timeout(timeout, connector.connect(server_name, socket))
            .await
            .context("Timeout during TLS handshake")??;

        let (reader, writer) = tokio::io::split(tls_stream);

        Ok(Self { reader: BufReader::new(reader), writer })
    }

    /// Sends a line to the server.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails.
    pub async fn send_line(&mut self, msg: &str) -> Result<()> {
        self.writer.write_all(msg.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        Ok(())
    }

    /// Reads a line from the server.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the read fails.
    pub async fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        Ok(line)
    }
}
