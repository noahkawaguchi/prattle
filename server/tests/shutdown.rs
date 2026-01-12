mod common;

use crate::common::{test_client::TestClient, test_server, tokio_test};
use anyhow::{Result, anyhow};
use std::time::Duration;

#[test]
fn shutdown_broadcasts_to_all_connected_clients() -> Result<()> {
    tokio_test(async {
        let (addr, shutdown_tx, _) = test_server::spawn_with_shutdown().await?;

        // Connect three clients
        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;
        let mut client2 = TestClient::connect_with_username("bob", &addr).await?;
        let mut client3 = TestClient::connect_with_username("charlie", &addr).await?;

        // Clear join messages
        client1.read_line_assert_contains("bob joined").await?;
        client1.read_line_assert_contains("charlie joined").await?;
        client2.read_line_assert_contains("charlie joined").await?;

        // Trigger shutdown
        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Give the server a moment to broadcast
        tokio::time::sleep(Duration::from_millis(50)).await;

        // All clients should receive the shutdown message (may be interleaved with "left" messages
        // as clients disconnect)
        client1
            .read_until_line_contains("Server is shutting down")
            .await?;
        client2
            .read_until_line_contains("Server is shutting down")
            .await?;
        client3
            .read_until_line_contains("Server is shutting down")
            .await?;

        Ok(())
    })
}

#[test]
fn shutdown_waits_for_clients_to_disconnect_gracefully() -> Result<()> {
    tokio_test(async {
        let (addr, shutdown_tx, server_handle) = test_server::spawn_with_shutdown().await?;

        let mut client = TestClient::connect_with_username("alice", &addr).await?;

        // Trigger shutdown
        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Read the shutdown message
        client
            .read_line_assert_contains("Server is shutting down")
            .await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Server should still be waiting for client to close connection (4s per-client timeout)
        assert!(
            !server_handle.is_finished(),
            "Server should still be waiting for client to disconnect"
        );

        // Now disconnect the client by dropping it
        drop(client);

        // Server should shut down quickly after client disconnects
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(
            server_handle.is_finished(),
            "Server should have shut down after client disconnected"
        );

        Ok(())
    })
}

#[test]
fn shutdown_times_out_when_client_does_not_disconnect() -> Result<()> {
    tokio_test(async {
        let (addr, shutdown_tx, server_handle) = test_server::spawn_with_shutdown().await?;

        let mut client = TestClient::connect_with_username("alice", &addr).await?;

        // Trigger shutdown
        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Client receives shutdown message but stays connected
        client
            .read_line_assert_contains("Server is shutting down")
            .await?;

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify server is still waiting before the 4s per-client timeout
        assert!(
            !server_handle.is_finished(),
            "Server should still be waiting for client (before client timeout)"
        );

        // Keep the client connected (don't drop it) and wait for the 4s per-client timeout
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Server should have shut down after the timeout despite the client still being connected
        assert!(
            server_handle.is_finished(),
            "Server should have shut down after 4s timeout despite client still being connected"
        );

        Ok(())
    })
}

#[test]
fn shutdown_proceeds_with_no_clients_ever() -> Result<()> {
    tokio_test(async {
        let (_, shutdown_tx, server_handle) = test_server::spawn_with_shutdown().await?;

        // Don't connect any clients and trigger shutdown immediately
        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Give the server time to shut down
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Server should have shut down cleanly
        assert!(
            server_handle.is_finished(),
            "Server should have shut down immediately with no clients"
        );

        Ok(())
    })
}

#[test]
fn shutdown_proceeds_if_all_clients_already_left() -> Result<()> {
    tokio_test(async {
        let (addr, shutdown_tx, server_handle) = test_server::spawn_with_shutdown().await?;

        // Connect two clients, drop them, and then shut down
        let client1 = TestClient::connect_with_username("alice", &addr).await?;
        let client2 = TestClient::connect_with_username("bob", &addr).await?;
        drop(client1);
        drop(client2);

        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Server should proceed to shutdown after at most one polling interval (100ms)
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(
            server_handle.is_finished(),
            "Server should have shut down immediately since all clients already left"
        );

        Ok(())
    })
}

#[test]
fn server_stops_accepting_new_connections_during_graceful_shutdown() -> Result<()> {
    tokio_test(async {
        let (addr, shutdown_tx, server_handle) = test_server::spawn_with_shutdown().await?;
        let mut client1 = TestClient::connect_with_username("alice", &addr).await?;

        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Client receives shutdown message but does not immediately disconnect
        client1
            .read_line_assert_contains("Server is shutting down")
            .await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        // The server should still be waiting for the first client to disconnect
        assert!(
            !server_handle.is_finished(),
            "Server should still be running (waiting for clients to disconnect)"
        );

        // A new connection attempt should fail while the server is in the graceful shutdown process
        // waiting for clients to disconnect
        assert!(
            TestClient::connect_with_username("bob", &addr)
                .await
                .is_err()
        );

        // Even after the connection attempt failure, the server should still be waiting for the
        // first client to disconnect
        assert!(
            !server_handle.is_finished(),
            "Server should still be running (waiting for clients to disconnect)"
        );

        Ok(())
    })
}

#[test]
fn shutdown_during_username_selection_disconnects_gracefully() -> Result<()> {
    tokio_test(async {
        let (addr, shutdown_tx, server_handle) = test_server::spawn_with_shutdown().await?;

        // Connect but don't provide a username yet
        let mut client = TestClient::connect(&addr).await?;

        // Read the username prompt
        let prompt = client.read_prompt().await?;
        assert!(
            prompt.contains("Choose a username:"),
            "Expected username prompt, got: {prompt}"
        );

        // Trigger shutdown while still in username selection
        shutdown_tx
            .send(())
            .map_err(|()| anyhow!("Failed to send shutdown signal"))?;

        // Client should receive shutdown message even during username selection
        client
            .read_until_line_contains("Server is shutting down")
            .await?;

        // Client stays connected but doesn't close the connection
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify server is still waiting before the 4s per-client timeout
        assert!(
            !server_handle.is_finished(),
            "Server should still be waiting for client (before client timeout)"
        );

        // Keep the client connected (don't drop it) and wait for the 4s per-client timeout
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Server should have shut down after the timeout despite the client still being connected
        assert!(
            server_handle.is_finished(),
            "Server should have shut down after 4s timeout despite client still being connected"
        );

        Ok(())
    })
}
