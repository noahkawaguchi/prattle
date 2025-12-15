mod common;

use crate::common::tokio_test;
use anyhow::{Context, Result};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    time::timeout,
};

/// The amount of time to wait when reading from the server.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Helper struct to manage a test client connection.
struct TestClient {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl TestClient {
    /// Connects to the server without completing username selection.
    async fn connect(addr: &str) -> Result<Self> {
        let socket = TcpStream::connect(addr).await?;
        let (reader, writer) = socket.into_split();
        Ok(Self { reader: BufReader::new(reader), writer })
    }

    /// Connects to the server and completes username selection.
    async fn connect_with_username(addr: &str, username: &str) -> Result<Self> {
        let socket = TcpStream::connect(addr).await?;
        let (reader, writer) = socket.into_split();
        let mut client = Self { reader: BufReader::new(reader), writer };

        // Read the "Choose a username: " prompt (doesn't end with newline)
        let prompt = client.read_until_prompt().await?;

        assert!(
            prompt.contains("Choose a username"),
            "Expected username prompt, got: {prompt}"
        );

        // Send username
        client.send_line(username).await?;

        // Client receives their own join message, so consume it here
        client
            .read_line_assert_contains_all(&[username, "joined the server"])
            .await?;

        Ok(client)
    }

    /// Sends a line to the server.
    async fn send_line(&mut self, msg: &str) -> Result<()> {
        self.writer.write_all(msg.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        Ok(())
    }

    /// Reads from the server until a prompt ending (text ending with ": " but no newline).
    async fn read_until_prompt(&mut self) -> Result<String> {
        let mut buffer = Vec::new();

        let read_future = async {
            loop {
                let byte = self.reader.fill_buf().await?;

                if byte.is_empty() {
                    break;
                }

                buffer.extend_from_slice(byte);
                let len = byte.len();
                self.reader.consume(len);

                // Check for a prompt (ends with ": ")
                if buffer.len() >= 2 && buffer[buffer.len() - 2..] == [b':', b' '] {
                    break;
                }

                // Also check if we got a newline (for error messages)
                if buffer.last() == Some(&b'\n') {
                    break;
                }
            }

            Ok(String::from_utf8_lossy(&buffer).to_string())
        };

        timeout(READ_TIMEOUT, read_future)
            .await
            .context("Timeout reading prompt")?
    }

    /// Reads a line from the server with a timeout and asserts that it contains the specified
    /// substring.
    async fn read_line_assert_contains(&mut self, expected: &str) -> Result<String> {
        self.read_line_assert_contains_all(&[expected]).await
    }

    /// Reads a line from the server with a timeout and asserts that it contains all the specified
    /// substrings.
    async fn read_line_assert_contains_all(&mut self, expected: &[&str]) -> Result<String> {
        let mut line = String::new();

        timeout(READ_TIMEOUT, self.reader.read_line(&mut line))
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
}

/// Spawns the server on a random available port and returns the address.
async fn spawn_test_server() -> Result<String> {
    // Bind to port 0 to get a random available port and immediately drop the listener so the port
    // is available for the server to bind
    let addr = TcpListener::bind("127.0.0.1:0")
        .await?
        .local_addr()?
        .to_string();

    // Clone addr for the spawned task
    let server_addr = addr.clone();

    // Spawn the server in a background task
    tokio::spawn(async move {
        if let Err(e) = prattle::run_server(server_addr).await {
            eprintln!("Error running test server: {e}");
        }
    });

    // Give the server a moment to start and bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(addr)
}

#[test]
fn client_can_connect() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;
        TestClient::connect_with_username(&addr, "alice").await?;
        Ok(())
    })
}

#[test]
fn empty_usernames_are_rejected() -> Result<()> {
    tokio_test(async {
        let mut client = TestClient::connect(&spawn_test_server().await?).await?;

        // Send empty usernames and expect error messages
        for empty_username in [" ", "   ", "", "　", "\t"] {
            client.send_line(empty_username).await?;
            client.read_line_assert_contains("cannot be empty").await?;
        }

        // Now send a valid username and expect the join message
        client.send_line("alice").await?;
        client
            .read_line_assert_contains("alice joined the server")
            .await?;

        Ok(())
    })
}

#[test]
fn duplicate_usernames_are_rejected() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;
        let _client1 = TestClient::connect_with_username(&addr, "alice").await?;

        // Try to connect with same username
        let mut client2 = TestClient::connect(&addr).await?;
        client2.read_until_prompt().await?;
        client2.send_line("alice").await?;

        // Expect rejection
        client2.read_line_assert_contains("taken").await?;

        // Send a different username and expect success
        client2.send_line("bob").await?;
        client2
            .read_line_assert_contains("bob joined the server")
            .await?;

        Ok(())
    })
}

#[test]
fn join_message_broadcasts_to_all_clients() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;
        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;

        // When client 2 connects, client 1 should see the join message
        let _client2 = TestClient::connect_with_username(&addr, "bob").await?;
        client1.read_line_assert_contains("bob joined").await?;

        Ok(())
    })
}

#[test]
fn client_messages_broadcast_to_all_clients() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;

        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;
        let mut client2 = TestClient::connect_with_username(&addr, "bob").await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Client 1 sends a message
        client1.send_line("Hello everyone!").await?;

        // Client 1 receives their own message
        client1
            .read_line_assert_contains("alice: Hello everyone!")
            .await?;

        // Client 2 should also receive it
        client2
            .read_line_assert_contains("alice: Hello everyone!")
            .await?;

        // Client 2 sends a message
        client2.send_line("Hi alice!").await?;

        // Client 2 receives their own message
        client2.read_line_assert_contains("bob: Hi alice!").await?;

        // Client 1 should also receive it
        client1.read_line_assert_contains("bob: Hi alice!").await?;

        Ok(())
    })
}

#[test]
fn who_command_lists_online_users() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;

        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;
        let mut client2 = TestClient::connect_with_username(&addr, "bob").await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Client 1 uses /who command
        client1.send_line("/who").await?;

        // Should see list of users
        client1
            .read_line_assert_contains_all(&["Currently online:", "alice", "bob"])
            .await?;

        // Client 2 uses /who command
        client2.send_line("/who").await?;

        // Should see same list
        client2
            .read_line_assert_contains_all(&["Currently online:", "alice", "bob"])
            .await?;

        Ok(())
    })
}

#[test]
fn action_command_broadcasts_to_all_clients() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;

        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;
        let mut client2 = TestClient::connect_with_username(&addr, "bob").await?;
        let mut client3 = TestClient::connect_with_username(&addr, "charlie").await?;

        // Consume join messages
        client1.read_line_assert_contains("bob joined").await?;
        client1.read_line_assert_contains("charlie joined").await?;
        client2.read_line_assert_contains("charlie joined").await?;

        // Client 1 performs an action
        client1.send_line("/action waves hello").await?;

        // Clients 2 and 3 should see the action
        client2
            .read_line_assert_contains("alice waves hello")
            .await?;
        client3
            .read_line_assert_contains("alice waves hello")
            .await?;

        Ok(())
    })
}

#[test]
fn quit_command_sends_goodbye_message_and_broadcast() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;

        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;
        let mut client2 = TestClient::connect_with_username(&addr, "bob").await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Client 1 quits
        client1.send_line("/quit").await?;

        // Quitting client should receive goodbye message
        client1.read_line_assert_contains("Goodbye").await?;

        // Client 2 should see leave message
        client2.read_line_assert_contains("alice left").await?;

        Ok(())
    })
}

#[test]
fn empty_messages_are_ignored() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;

        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;
        let mut client2 = TestClient::connect_with_username(&addr, "bob").await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Send empty line
        client1.send_line("").await?;

        // Send a real message after
        client1.send_line("Real message").await?;

        // Client 2 should only see the real message
        client2
            .read_line_assert_contains("alice: Real message")
            .await?;

        Ok(())
    })
}

#[test]
fn multiple_clients_can_broadcast() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;

        let mut client1 = TestClient::connect_with_username(&addr, "alice").await?;
        let mut client2 = TestClient::connect_with_username(&addr, "bob").await?;
        let mut client3 = TestClient::connect_with_username(&addr, "charlie").await?;

        // Consume join messages received by each client
        client1.read_line_assert_contains("bob joined").await?;
        client1.read_line_assert_contains("charlie joined").await?;
        client2.read_line_assert_contains("charlie joined").await?;

        // Client 3 sends a message
        client3.send_line("Hello from charlie!").await?;

        // Everyone should see the message
        client1
            .read_line_assert_contains("charlie: Hello from charlie!")
            .await?;
        client2
            .read_line_assert_contains("charlie: Hello from charlie!")
            .await?;
        client3
            .read_line_assert_contains("charlie: Hello from charlie!")
            .await?;

        // Should work the same with a different sender
        client2.send_line("Hello from bob!").await?;

        client1
            .read_line_assert_contains("bob: Hello from bob!")
            .await?;
        client2
            .read_line_assert_contains("bob: Hello from bob!")
            .await?;
        client3
            .read_line_assert_contains("bob: Hello from bob!")
            .await?;

        Ok(())
    })
}
