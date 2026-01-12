use anyhow::{Context, Result};
use prattle_client::client_connection::ClientConnection;
use std::time::Duration;
use tokio::time::Instant;

/// The amount of time to wait when connecting to the server.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// The amount of time to wait when reading from the server.
const READ_TIMEOUT: Duration = Duration::from_secs(1);

/// Helper struct to manage a test client connection.
pub struct TestClient {
    conn: ClientConnection,
}

impl TestClient {
    /// Connects to the server without completing username selection.
    pub async fn connect(addr: &str) -> Result<Self> {
        Ok(Self { conn: ClientConnection::connect(addr, CONNECT_TIMEOUT).await? })
    }

    /// Connects to the server and completes username selection.
    pub async fn connect_with_username(username: &str, addr: &str) -> Result<Self> {
        let mut client = Self::connect(addr).await?;

        // Read the "Choose a username: " prompt (doesn't end with newline)
        let prompt = client.read_prompt().await?;

        assert!(
            prompt.contains("Choose a username:"),
            "Expected username prompt, got: {prompt}"
        );

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
    pub async fn send_line(&mut self, msg: &str) -> Result<()> { self.conn.send_line(msg).await }

    /// Reads a prompt from the server using custom termination logic.
    ///
    /// Specifically, reads until the first ':', then also reads the following byte (assumed to be a
    /// trailing space).
    pub async fn read_prompt(&mut self) -> Result<String> {
        self.conn.read_prompt(READ_TIMEOUT).await
    }

    /// Reads a line from the server with a timeout and asserts that it contains the specified
    /// substring.
    pub async fn read_line_assert_contains(&mut self, expected: &str) -> Result<String> {
        self.read_line_assert_contains_all(&[expected]).await
    }

    /// Reads a line from the server with a timeout and asserts that it contains all the specified
    /// substrings.
    pub async fn read_line_assert_contains_all(&mut self, expected: &[&str]) -> Result<String> {
        let line = tokio::time::timeout(READ_TIMEOUT, self.conn.read_line())
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

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());

            let line = tokio::time::timeout(remaining, self.conn.read_line())
                .await
                .context("Timeout reading lines until match")??;

            if line.contains(expected) {
                break Ok(line);
            }
        }
    }
}
