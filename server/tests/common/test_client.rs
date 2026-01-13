use anyhow::{Context, Result};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    time::Instant,
};

/// The amount of time to wait when connecting to the server.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// The amount of time to wait when reading from the server.
const READ_TIMEOUT: Duration = Duration::from_secs(1);

/// Helper struct to manage a test client connection.
pub struct TestClient {
    reader: prattle_client::ClientReader,
    writer: prattle_client::ClientWriter,
}

impl TestClient {
    /// Connects to the server without completing username selection.
    pub async fn connect(addr: &str) -> Result<Self> {
        let (reader, writer) = prattle_client::connect(addr, CONNECT_TIMEOUT).await?;
        Ok(Self { reader, writer })
    }

    /// Connects to the server and completes username selection.
    pub async fn connect_with_username(username: &str, addr: &str) -> Result<Self> {
        let mut client = Self::connect(addr).await?;

        // Read the "Choose a username:" prompt
        client
            .read_line_assert_contains_all(&["Choose", "username"])
            .await?;

        // Send username
        client.send_line(username).await?;

        // Client receives a welcome message and their own join message
        client
            .read_line_assert_contains_all(&[username, "welcome"])
            .await?;
        client
            .read_line_assert_contains_all(&[username, "joined the server"])
            .await?;

        Ok(client)
    }

    /// Sends a line to the server.
    pub async fn send_line(&mut self, msg: &str) -> Result<()> {
        self.writer.write_all(msg.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        Ok(())
    }

    /// Reads a line from the server with a timeout and asserts that it contains the specified
    /// substring.
    pub async fn read_line_assert_contains(&mut self, expected: &str) -> Result<String> {
        self.read_line_assert_contains_all(&[expected]).await
    }

    /// Reads a line from the server with a timeout and asserts that it contains all the specified
    /// substrings.
    pub async fn read_line_assert_contains_all(&mut self, expected: &[&str]) -> Result<String> {
        let mut line = String::new();

        tokio::time::timeout(READ_TIMEOUT, self.reader.read_line(&mut line))
            .await
            .context("Timeout reading line")??;

        for substr in expected {
            assert!(
                line.contains(substr),
                "Expected line to contain \"{substr}\", got: \"{line}\""
            );
        }

        Ok(line)
    }

    /// Reads lines from the server until one contains the expected substring, or times out.
    ///
    /// This is useful when other messages might be arbitrarily interleaved (e.g., "left the server"
    /// messages when multiple clients get disconnected at the same time during shutdown).
    #[allow(dead_code)] // Not actually dead code
    pub async fn read_until_line_contains(&mut self, expected: &str) -> Result<String> {
        let deadline = Instant::now() + READ_TIMEOUT;
        let mut line = String::new();

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());

            tokio::time::timeout(remaining, self.reader.read_line(&mut line))
                .await
                .context("Timeout reading lines until match")??;

            if line.contains(expected) {
                break Ok(line);
            }

            line.clear();
        }
    }

    /// Reads to the end of the reader half with a timeout to expect the server's `close_notify`,
    /// gracefully closes the writer half of the connection to send `close_notify`, and consumes
    /// `self`.
    #[allow(dead_code)] // Not actually dead code
    pub async fn graceful_disconnect(mut self) -> Result<()> {
        let mut discard = Vec::new();

        tokio::time::timeout(READ_TIMEOUT, self.reader.read_to_end(&mut discard))
            .await
            .context("Timeout reading until EOF")??;

        self.writer.shutdown().await?;

        Ok(())
    }
}
