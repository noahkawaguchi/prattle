use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    time::timeout,
};

/// Helper struct to manage a test client connection
struct TestClient {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl TestClient {
    /// Connect to the server and complete username selection
    async fn connect(addr: &str, username: &str) -> anyhow::Result<Self> {
        let socket = TcpStream::connect(addr).await?;
        let (reader, writer) = socket.into_split();
        let mut client = TestClient { reader: BufReader::new(reader), writer };

        // Read the "Choose a username: " prompt (doesn't end with newline)
        let prompt = client.read_until_prompt().await?;
        assert!(prompt.contains("Choose a username"), "Expected username prompt, got: {}", prompt);

        // Send username
        client.send_line(username).await?;

        // Client receives their own join message - consume it
        client.expect_line_contains("joined the server").await?;

        Ok(client)
    }

    /// Send a line to the server
    async fn send_line(&mut self, msg: &str) -> anyhow::Result<()> {
        self.writer.write_all(msg.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        Ok(())
    }

    /// Read until we get a prompt (text ending with ": " but no newline)
    async fn read_until_prompt(&mut self) -> anyhow::Result<String> {
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

                // Check if we have a prompt (ends with ": ")
                if buffer.len() >= 2 && buffer[buffer.len() - 2..] == [b':', b' '] {
                    break;
                }

                // Also check if we got a newline (for error messages)
                if buffer.last() == Some(&b'\n') {
                    break;
                }
            }
            Ok::<_, anyhow::Error>(String::from_utf8_lossy(&buffer).to_string())
        };

        timeout(Duration::from_secs(2), read_future)
            .await
            .map_err(|_| anyhow::anyhow!("Timeout reading prompt"))?
    }

    /// Read a single line from the server with a timeout
    async fn read_line(&mut self) -> anyhow::Result<String> {
        let mut line = String::new();
        timeout(Duration::from_secs(2), self.reader.read_line(&mut line))
            .await
            .map_err(|_| anyhow::anyhow!("Timeout reading line"))??;
        Ok(line)
    }

    /// Read a line and expect it to contain a specific substring
    async fn expect_line_contains(&mut self, expected: &str) -> anyhow::Result<String> {
        let line = self.read_line().await?;
        assert!(
            line.contains(expected),
            "Expected line to contain '{}', got: '{}'",
            expected,
            line
        );
        Ok(line)
    }
}

/// Spawn the server on a random available port and return the address
async fn spawn_test_server() -> anyhow::Result<String> {
    // Bind to port 0 to get a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();

    // Drop the listener so the port is available for the server to bind
    drop(listener);

    // Clone addr for the spawned task
    let server_addr = addr.clone();

    // Spawn the server in a background task using the library function
    tokio::spawn(async move {
        // Ignore errors from server shutdown
        let _ = prattle::run_server(server_addr).await;
    });

    // Give the server a moment to start and bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(addr)
}

#[tokio::test]
async fn test_single_client_connection() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;
    let _client = TestClient::connect(&addr, "alice").await?;
    Ok(())
}

#[tokio::test]
async fn test_username_cannot_be_empty() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;
    let socket = TcpStream::connect(&addr).await?;
    let (reader, writer) = socket.into_split();
    let mut client = TestClient { reader: BufReader::new(reader), writer };

    // Read initial prompt
    client.read_until_prompt().await?;

    // Send empty username
    client.send_line("").await?;

    // Expect error message
    client.expect_line_contains("cannot be empty").await?;

    // Now send valid username
    client.send_line("alice").await?;

    Ok(())
}

#[tokio::test]
async fn test_duplicate_username_rejected() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let _client1 = TestClient::connect(&addr, "alice").await?;

    // Try to connect with same username
    let socket = TcpStream::connect(&addr).await?;
    let (reader, writer) = socket.into_split();
    let mut client2 = TestClient { reader: BufReader::new(reader), writer };

    // Read initial prompt
    client2.read_until_prompt().await?;

    // Send duplicate username
    client2.send_line("alice").await?;

    // Expect rejection
    client2.expect_line_contains("taken").await?;

    // Send different username
    client2.send_line("bob").await?;

    Ok(())
}

#[tokio::test]
async fn test_join_message_broadcast() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;

    // Client 2 connects - client 1 should see the join message
    let _client2 = TestClient::connect(&addr, "bob").await?;

    // Client 1 should receive bob's join message
    client1.expect_line_contains("bob joined").await?;

    Ok(())
}

#[tokio::test]
async fn test_message_broadcasting() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;
    let mut client2 = TestClient::connect(&addr, "bob").await?;

    // Client 1 should receive bob's join message
    client1.expect_line_contains("bob joined").await?;

    // Client 1 sends a message
    client1.send_line("Hello everyone!").await?;

    // Client 1 receives their own message
    client1.expect_line_contains("alice: Hello everyone!").await?;

    // Client 2 should also receive it
    client2.expect_line_contains("alice: Hello everyone!").await?;

    // Client 2 sends a message
    client2.send_line("Hi alice!").await?;

    // Client 2 receives their own message
    client2.expect_line_contains("bob: Hi alice!").await?;

    // Client 1 should also receive it
    client1.expect_line_contains("bob: Hi alice!").await?;

    Ok(())
}

#[tokio::test]
async fn test_who_command() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;
    let mut client2 = TestClient::connect(&addr, "bob").await?;

    // Client 1 should receive bob's join message
    client1.expect_line_contains("bob joined").await?;

    // Client 1 uses /who command
    client1.send_line("/who").await?;

    // Should see list of users
    let response = client1.read_line().await?;
    assert!(response.contains("Currently online:"));
    assert!(response.contains("alice"));
    assert!(response.contains("bob"));

    // Client 2 uses /who command
    client2.send_line("/who").await?;

    // Should see same list
    let response = client2.read_line().await?;
    assert!(response.contains("Currently online:"));
    assert!(response.contains("alice"));
    assert!(response.contains("bob"));

    Ok(())
}

#[tokio::test]
async fn test_action_command() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;
    let mut client2 = TestClient::connect(&addr, "bob").await?;

    // Client 1 should receive bob's join message
    client1.expect_line_contains("bob joined").await?;

    // Client 1 performs an action
    client1.send_line("/action waves hello").await?;

    // Client 2 should see the action
    client2.expect_line_contains("alice waves hello").await?;

    Ok(())
}

#[tokio::test]
async fn test_quit_command() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;
    let mut client2 = TestClient::connect(&addr, "bob").await?;

    // Client 1 should receive bob's join message
    client1.expect_line_contains("bob joined").await?;

    // Client 1 quits
    client1.send_line("/quit").await?;

    // Should receive goodbye message
    client1.expect_line_contains("Goodbye").await?;

    // Client 2 should see leave message
    client2.expect_line_contains("alice left").await?;

    Ok(())
}

#[tokio::test]
async fn test_empty_lines_ignored() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;
    let mut client2 = TestClient::connect(&addr, "bob").await?;

    // Client 1 should receive bob's join message
    client1.expect_line_contains("bob joined").await?;

    // Send empty line
    client1.send_line("").await?;

    // Send a real message after
    client1.send_line("Real message").await?;

    // Client 2 should only see the real message
    client2.expect_line_contains("alice: Real message").await?;

    Ok(())
}

#[tokio::test]
async fn test_multiple_clients_broadcasting() -> anyhow::Result<()> {
    let addr = spawn_test_server().await?;

    let mut client1 = TestClient::connect(&addr, "alice").await?;
    let mut client2 = TestClient::connect(&addr, "bob").await?;
    let mut client3 = TestClient::connect(&addr, "charlie").await?;

    // Consume join messages received by each client
    client1.expect_line_contains("bob joined").await?;
    client1.expect_line_contains("charlie joined").await?;
    client2.expect_line_contains("charlie joined").await?;

    // Client 3 sends a message
    client3.send_line("Hello from charlie!").await?;

    // Both client 1 and 2 should receive it
    client1.expect_line_contains("charlie: Hello from charlie!").await?;
    client2.expect_line_contains("charlie: Hello from charlie!").await?;

    Ok(())
}
