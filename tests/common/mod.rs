pub mod test_client;

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::net::TcpListener;
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

/// Spawns the server on a random available port and returns the address.
pub async fn spawn_test_server() -> Result<String> {
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
    tokio::spawn(async move {
        match prattle::shutdown_signal_handler() {
            Err(e) => error!("Error installing shutdown signal handler: {e}"),
            Ok(shutdown_signal) => {
                if let Err(e) = prattle::run_server(&server_addr, shutdown_signal).await {
                    error!("Error running test server: {e}");
                }
            }
        }
    });

    // Give the server a moment to start and bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(addr)
}

/// Initializes a tracing subscriber for tests at the default "error" level (unless overridden by
/// `RUST_LOG`), ignoring the error if the subscriber was already initialized.
fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}
