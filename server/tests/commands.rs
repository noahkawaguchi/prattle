mod common;

use crate::common::{test_client::TestClient, test_server, tokio_test};
use anyhow::Result;

#[test]
fn quit_command_sends_goodbye_message_and_broadcast() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Client 1 quits
        client1.send_line("/quit").await?;

        // Quitting client should receive goodbye message
        client1.read_line_assert_contains("Goodbye").await?;

        // Quitting client disconnects
        drop(client1);

        // Client 2 should see leave message
        client2.read_line_assert_contains("alice left").await?;

        Ok(())
    })
}

#[test]
fn help_command_lists_usage() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Client 1 uses /help command
        client1.send_line("/help").await?;

        // Should see the help block
        let help_words = ["", "quit", "help", "who", "action", "", "message", ""];
        for word in help_words {
            client1.read_line_assert_contains(word).await?;
        }

        // Client 2 should not have seen Client 1's help message
        assert!(client2.read_line_assert_contains("").await.is_err());

        // Client 2 should get the same block after using the /help command
        client2.send_line("/help").await?;
        for word in help_words {
            client2.read_line_assert_contains(word).await?;
        }

        Ok(())
    })
}

#[test]
fn who_command_lists_online_users() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;

        // Client 1 should receive bob's join message
        client1.read_line_assert_contains("bob joined").await?;

        // Client 1 uses /who command
        client1.send_line("/who").await?;

        // Should see list of users
        client1
            .read_line_assert_contains_all(&["Currently online:", "alice", "bob"])
            .await?;

        // Client 2 should not have seen Client 1's listing
        assert!(client2.read_line_assert_contains("").await.is_err());

        // Client 2 should get the same list after using the /help command
        client2.send_line("/who").await?;
        client2
            .read_line_assert_contains_all(&["Currently online:", "alice", "bob"])
            .await?;

        // Users who quit should not be included in the /who command listing
        client1.send_line("/quit").await?;
        client1.read_line_assert_contains("Goodbye").await?;
        drop(client1);
        client2.read_line_assert_contains("alice left").await?;
        client2.send_line("/who").await?;
        let who_listing = client2
            .read_line_assert_contains("Currently online")
            .await?;
        assert!(!who_listing.contains("alice"));

        Ok(())
    })
}

#[test]
fn action_command_broadcasts_to_all_clients() -> Result<()> {
    tokio_test(async {
        let addr = test_server::spawn().await?;

        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;
        let mut client3 = TestClient::connect_with_username("charlie", &addr).await?;

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
