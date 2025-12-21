use crate::common::TEST_LOG_LEVEL;
use anyhow::Result;
use std::time::Duration;
use tokio::{
    net::TcpListener,
    sync::oneshot::{self, Sender},
    task::JoinHandle,
};

/// Spawns the server on a random available port, returning the address, a `Sender` to send the
/// shutdown signal, and a `JoinHandle` to the server task.
#[allow(dead_code)] // Not actually dead code
pub async fn spawn_with_shutdown() -> Result<(String, Sender<()>, JoinHandle<()>)> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let (addr, handle) = inner_spawn_with_shutdown(async {
        shutdown_rx.await.ok();
    })
    .await?;

    Ok((addr, shutdown_tx, handle))
}

/// Spawns the server with the default signal handler on a random available port and returns the
/// address.
#[allow(dead_code)] // Not actually dead code
pub async fn spawn() -> Result<String> {
    let (addr, _) = inner_spawn_with_shutdown(prattle::shutdown_signal::listen()?).await?;

    Ok(addr)
}

/// Spawns the server with `shutdown_signal` as the shutdown signal on a random available port and
/// returns the address and a `JoinHandle` to the server task.
async fn inner_spawn_with_shutdown(
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<(String, JoinHandle<()>)> {
    // Ignore the error if the tracing subscriber was already initialized in another test
    let _ = prattle::logger::init_with_default(TEST_LOG_LEVEL);

    // Bind to port 0 to get a random available port and immediately drop the listener so the port
    // is available for the server to bind
    let addr = TcpListener::bind("127.0.0.1:0")
        .await?
        .local_addr()?
        .to_string();

    // Clone addr for the spawned task
    let server_addr = addr.clone();

    // Create TLS configuration for the test server
    let tls_config = prattle::tls::create_config()?;

    // Spawn the server in a background task
    let handle = tokio::spawn(async move {
        if let Err(e) = prattle::server::run(&server_addr, tls_config, shutdown_signal).await {
            // `eprintln!` instead of `error!` because logging may be off in tests
            eprintln!("Error running test server: {e}");
        }
    });

    // Give the server a moment to start and bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok((addr, handle))
}
