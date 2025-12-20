pub(crate) mod client;
pub(crate) mod command;

use anyhow::Result;
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{
        Mutex,
        broadcast::{self},
    },
};
use tracing::{error, info, warn};

/// The number of messages that can be held in the channel.
const CHANNEL_CAP: usize = 100;

/// The time to wait for clients to disconnect during graceful shutdown.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs the chat server on the specified bind address.
///
/// Specifically:
///
/// - Binds a TCP listener to the provided address
/// - Accepts incoming client connections
/// - Handles messages, commands, and broadcasting between clients
/// - Gracefully shuts down upon receiving a shutdown signal
///
/// # Errors
///
/// Returns `Err` for any errors with the overall operation of the server, but logs and does not
/// return errors from handling specific clients.
pub async fn run_server(bind_addr: &str) -> Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    info!("Listening on {bind_addr}");

    let (sender, _) = broadcast::channel(CHANNEL_CAP);
    let (shutdown_tx, _) = broadcast::channel(1);
    let users = Arc::new(Mutex::new(HashSet::new()));

    let shutdown_signal = shutdown_signal_handler()?;
    tokio::pin!(shutdown_signal);

    if loop {
        tokio::select! {
            conn_result = listener.accept() => {
                let (socket, client_addr) = conn_result?;
                info!("New connection from {client_addr}");

                let tx = sender.clone();
                let rx = tx.subscribe();
                let users_clone = Arc::clone(&users);
                let shutdown_rx = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    match client::handle_client(socket, tx, rx, shutdown_rx, users_clone).await {
                        Err(e) => error!("Error handling client {client_addr}: {e}"),
                        Ok(()) => info!("Client {client_addr} disconnected"),
                    }
                });
            }

            () = &mut shutdown_signal => {
                break match shutdown_tx.send(()) {
                    Ok(receivers) => {
                        info!("Broadcast shutdown to {receivers} client(s)");
                        true
                    }
                    Err(e) if users.lock().await.is_empty() => {
                        info!("No users online to broadcast shutdown to: {e}");
                        false
                    }
                    Err(e) => {
                        error!("Failed to broadcast shutdown with users online: {e}");
                        false
                    }
                }
            }
        }
    } {
        info!("Waiting for clients to disconnect");

        let start = Instant::now();

        while !users.lock().await.is_empty() {
            if start.elapsed() >= SHUTDOWN_TIMEOUT {
                let remaining = users.lock().await.len();
                warn!("Shutdown timeout reached with {remaining} client(s) still connected");
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    info!("Server shutting down now");
    Ok(())
}

/// Creates Unix signal handlers that listen for SIGINT and SIGTERM.
#[cfg(unix)]
fn shutdown_signal_handler() -> Result<impl std::future::Future<Output = ()>> {
    use tokio::signal::unix;

    let mut sigint = unix::signal(unix::SignalKind::interrupt())?;
    let mut sigterm = unix::signal(unix::SignalKind::terminate())?;

    Ok(async move {
        tokio::select! {
            v = sigint.recv() => {
                match v {
                    Some(()) => info!("SIGINT received, shutting down..."),
                    None => warn!("SIGINT stream ended unexpectedly, shutting down..."),
                }
            }
            v = sigterm.recv() => {
                match v {
                    Some(()) => info!("SIGTERM received, shutting down..."),
                    None => warn!("SIGTERM stream ended unexpectedly, shutting down..."),
                }
            }
        }
    })
}

/// Creates a cross-platform signal handler that listens for Ctrl+C.
#[allow(clippy::unnecessary_wraps)] // Wrapped in `Result` to match the Unix version
#[cfg(not(unix))]
fn shutdown_signal_handler() -> Result<impl std::future::Future<Output = ()>> {
    Ok(async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => info!("Ctrl+C received, shutting down..."),
            Err(e) => warn!("Ctrl+C handler error, shutting down: {e}"),
        }
    })
}
