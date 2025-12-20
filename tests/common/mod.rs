pub mod test_client;

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::{
    net::TcpListener,
    sync::oneshot::{self, Sender},
    task::JoinHandle,
};
use tracing::error;
use tracing_subscriber::EnvFilter;

/// Replaces `#[tokio::test]`, not inserting `#[allow(clippy::expect_used)]`.
///
/// Based on the "equivalent code" listed in the docs at
/// <https://docs.rs/tokio/latest/tokio/attr.test.html#using-current-thread-runtime>
pub fn tokio_test<F: Future<Output = Result<()>>>(f: F) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to set up Tokio runtime for test")?
        .block_on(f)
}

/// Spawns the server on a random available port, returning the address, a `Sender` to send the
/// shutdown signal, and a `JoinHandle` to the server task.
pub async fn spawn_test_server_with_shutdown() -> Result<(String, Sender<()>, JoinHandle<()>)> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let (addr, handle) = inner_spawn_test_server_with_shutdown(async {
        shutdown_rx.await.ok();
    })
    .await?;

    Ok((addr, shutdown_tx, handle))
}

/// Spawns the server with the default signal handler on a random available port and returns the
/// address.
pub async fn spawn_test_server() -> Result<String> {
    let (addr, _) =
        inner_spawn_test_server_with_shutdown(prattle::shutdown_signal::listen()?).await?;

    Ok(addr)
}

/// Spawns the server with `shutdown_signal` as the shutdown signal on a random available port and
/// returns the address and a `JoinHandle` to the server task.
async fn inner_spawn_test_server_with_shutdown(
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<(String, JoinHandle<()>)> {
    init_test_tracing();

    // Bind to port 0 to get a random available port and immediately drop the listener so the port
    // is available for the server to bind
    let addr = TcpListener::bind("127.0.0.1:0")
        .await?
        .local_addr()?
        .to_string();

    // Clone addr for the spawned task
    let server_addr = addr.clone();

    // Spawn the server in a background task
    let handle = tokio::spawn(async move {
        if let Err(e) = prattle::server::run(&server_addr, shutdown_signal).await {
            error!("Error running test server: {e}");
        }
    });

    // Give the server a moment to start and bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok((addr, handle))
}

/// Initializes a tracing subscriber for tests at the default "error" level (unless overridden by
/// `RUST_LOG`), ignoring the error if the subscriber was already initialized.
fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}
