mod common;

use crate::common::{test_client::TestClient, test_server, tokio_test};
use anyhow::Result;

#[test]
fn client_messages_broadcast_to_all_clients() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;

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
fn multiple_clients_can_broadcast_messages() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;
        let mut client3 = TestClient::connect_with_username("charlie", &addr).await?;

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

#[test]
fn empty_messages_are_ignored() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;

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
