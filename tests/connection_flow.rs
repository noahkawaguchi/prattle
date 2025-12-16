mod common;

use crate::common::{spawn_test_server, test_client::TestClient, tokio_test};
use anyhow::Result;

#[test]
fn client_can_connect() -> Result<()> {
    tokio_test(async {
        let addr = spawn_test_server().await?;
        TestClient::connect_with_username("alice", &addr).await?;
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

        // Now send a valid username and expect the welcome/join messages
        client.send_line("alice").await?;
        client
            .read_line_assert_contains_all(&["alice", "welcome"])
            .await?;
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
        let _client1 = TestClient::connect_with_username("alice", &addr).await?;

        // Try to connect with same username
        let mut client2 = TestClient::connect(&addr).await?;
        client2.read_prompt().await?;
        client2.send_line("alice").await?;

        // Expect rejection
        client2.read_line_assert_contains("taken").await?;

        // Send a different username and expect success
        client2.send_line("bob").await?;
        client2
            .read_line_assert_contains_all(&["bob", "welcome"])
            .await?;
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
        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;

        // When client 2 connects, client 1 should see the join message
        let _client2 = TestClient::connect_with_username("bob", &addr).await?;
        client1.read_line_assert_contains("bob joined").await?;

        Ok(())
    })
}
