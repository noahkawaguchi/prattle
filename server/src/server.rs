use crate::client;
use anyhow::Result;
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, broadcast},
};
use tokio_rustls::{TlsAcceptor, rustls::ServerConfig};
use tracing::{error, info, warn};

/// The number of messages that can be held in the channel.
const CHANNEL_CAP: usize = 100;

/// The time to wait for all clients to disconnect during graceful shutdown.
pub(crate) const GLOBAL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs the chat server on `bind_addr` using TLS as configured with `tls_config` until receiving
/// `shutdown_signal`.
///
/// Specifically:
///
/// - Binds a TCP listener to the provided address
/// - Accepts incoming client connections with TLS encryption
/// - Handles messages, commands, and broadcasting between clients
/// - Gracefully shuts down upon receiving a shutdown signal
///
/// # Errors
///
/// Returns `Err` for any errors with the overall operation of the server, but logs and does not
/// return errors from handling specific clients.
pub async fn run(
    bind_addr: &str,
    tls_config: Arc<ServerConfig>,
    shutdown_signal: impl Future<Output = ()>,
) -> Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    let tls_acceptor = TlsAcceptor::from(tls_config);
    info!("Listening on {bind_addr}");

    let (sender, _) = broadcast::channel(CHANNEL_CAP);
    let (shutdown_tx, _) = broadcast::channel(1);
    // All client connections, regardless of whether they have provided a username
    let active_clients = Arc::new(AtomicUsize::new(0));
    // The set of usernames provided by active clients
    let users = Arc::new(Mutex::new(HashSet::new()));

    tokio::pin!(shutdown_signal);

    if loop {
        tokio::select! {
            conn_result = listener.accept() => {
                let (socket, client_addr) = conn_result?;
                info!("New connection from {client_addr}");

                let acceptor = tls_acceptor.clone();
                let tx = sender.clone();
                let rx = tx.subscribe();
                let users_clone = Arc::clone(&users);
                let active_clients_clone = Arc::clone(&active_clients);
                let shutdown_rx = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    match acceptor.accept(socket).await {
                        Err(e) => error!("TLS handshake failed for {client_addr}: {e}"),

                        Ok(tls_stream) => {
                            info!("TLS handshake completed for {client_addr}");

                            active_clients_clone.fetch_add(1, SeqCst);

                            if let Err(e) =
                                client::handle_client(tls_stream, tx, rx, shutdown_rx, users_clone)
                                    .await
                            {
                                error!("Error handling client {client_addr}: {e}");
                            } else {
                                info!("Client {client_addr} disconnected");
                            }

                            active_clients_clone.fetch_sub(1, SeqCst);
                        }
                    }
                });
            }

            () = &mut shutdown_signal => {
                break match shutdown_tx.send(()) {
                    Ok(receivers) => {
                        info!("Broadcast shutdown to {receivers} client(s)");
                        true
                    }
                    Err(e) if users.lock().await.is_empty() && active_clients.load(SeqCst) == 0 => {
                        warn!("No users online to broadcast shutdown to: {e}");
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

        while !users.lock().await.is_empty() || active_clients.load(SeqCst) > 0 {
            if start.elapsed() >= GLOBAL_SHUTDOWN_TIMEOUT {
                warn!(
                    "Global shutdown timeout reached with {} user(s) and \
                    {} active client(s) still connected",
                    users.lock().await.len(),
                    active_clients.load(SeqCst)
                );

                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    info!("Server shutting down now");
    Ok(())
}
