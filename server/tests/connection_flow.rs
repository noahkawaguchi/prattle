mod common;

use crate::common::{test_client::TestClient, test_server, tokio_test};
use anyhow::Result;

#[test]
fn client_can_connect() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;
        TestClient::connect_with_username("alice", &addr).await?;
        Ok(())
    })
}

#[test]
fn empty_usernames_are_rejected() -> Result<()> {
    tokio_test(async {
        let mut client = TestClient::connect(&test_server::spawn().await?).await?;

        // Send empty usernames and expect error messages
        for empty_username in [" ", "   ", "", "　", "\t"] {
            client
                .read_line_assert_contains_all(&["Choose", "username"])
                .await?;
            client.send_line(empty_username).await?;
            client.read_line_assert_contains("cannot be empty").await?;
        }

        // Now send a valid username and expect the welcome/join messages
        client
            .read_line_assert_contains_all(&["Choose", "username"])
            .await?;
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
fn using_the_unknown_username_is_rejected() -> Result<()> {
    tokio_test(async {
        let mut client = TestClient::connect(&test_server::spawn().await?).await?;

        // Attempt to join with the literal string used for unknown usernames and expect an error
        // message
        client
            .read_line_assert_contains_all(&["Choose", "username"])
            .await?;
        client.send_line("[unknown]").await?;
        client.read_line_assert_contains("Invalid username").await?;

        // Now send a valid username and expect the welcome/join messages
        client
            .read_line_assert_contains_all(&["Choose", "username"])
            .await?;
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
        let addr = test_server::spawn().await?;
        let _client1 = TestClient::connect_with_username("alice", &addr).await?;

        // Try to connect with same username
        let mut client2 = TestClient::connect(&addr).await?;
        client2
            .read_line_assert_contains_all(&["Choose", "username"])
            .await?;
        client2.send_line("alice").await?;

        // Expect rejection
        client2.read_line_assert_contains("taken").await?;

        // Send a different username and expect success
        client2
            .read_line_assert_contains_all(&["Choose", "username"])
            .await?;
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
        let addr = test_server::spawn().await?;
        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;

        // When client 2 connects, client 1 should see the join message
        let _client2 = TestClient::connect_with_username("bob", &addr).await?;
        client1.read_line_assert_contains("bob joined").await?;

        Ok(())
    })
}
