mod client_conn;

use anyhow::Result;
use std::{
    collections::HashSet,
    env,
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

const CHANNEL_CAP: usize = 100;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

type Users = Arc<Mutex<HashSet<String>>>;

async fn async_main() -> Result<()> {
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Listening on {bind_addr}");

    let (sender, _) = broadcast::channel(CHANNEL_CAP);
    let (shutdown_tx, _) = broadcast::channel(1);
    let users = Arc::new(Mutex::new(HashSet::new()));

    let shutdown_signal = shutdown_signal_handler()?;
    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            conn_result = listener.accept() => {
                let (socket, client_addr) = conn_result?;
                println!("New connection from {client_addr}");

                let tx = sender.clone();
                let rx = tx.subscribe();
                let users_clone = Arc::clone(&users);
                let shutdown_rx = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    match client_conn::handle_client(socket, tx, rx, shutdown_rx, users_clone).await
                    {
                        Err(e) => eprintln!("Error handling client {client_addr}: {e}"),
                        Ok(()) => println!("Client {client_addr} disconnected"),
                    }
                });
            }

            () = &mut shutdown_signal => {
                println!("Shutdown signal received, notifying clients");

                if let Err(e) = shutdown_tx.send(()) {
                    eprintln!("Failed to broadcast shutdown signal: {e}");
                }

                break;
            }
        }
    }

    let start = Instant::now();

    while !users.lock().await.is_empty() {
        if start.elapsed() >= SHUTDOWN_TIMEOUT {
            let remaining = users.lock().await.len();
            println!("Shutdown timeout reached with {remaining} client(s) still connected");
            break;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    println!("Graceful shutdown process done. Shutting down now.");
    Ok(())
}

/// Creates Unix signal handlers that listen for SIGINT and SIGTERM.
#[cfg(unix)]
fn shutdown_signal_handler() -> Result<impl Future<Output = ()>> {
    use tokio::signal::unix;

    let mut sigint = unix::signal(unix::SignalKind::interrupt())?;
    let mut sigterm = unix::signal(unix::SignalKind::terminate())?;

    Ok(async move {
        tokio::select! {
            v = sigint.recv() => {
                match v {
                    Some(()) => println!("\nSIGINT received, shutting down..."),
                    None => eprintln!("\nSIGINT stream ended unexpectedly, shutting down..."),
                }
            }
            v = sigterm.recv() => {
                match v {
                    Some(()) => println!("SIGTERM received, shutting down..."),
                    None => eprintln!("SIGTERM stream ended unexpectedly, shutting down..."),
                }
            }
        }
    })
}

/// Creates a cross-platform signal handler that listens for Ctrl+C.
#[allow(clippy::unnecessary_wraps)] // Wrapped in `Result` to match the Unix version
#[cfg(not(unix))]
fn shutdown_signal_handler() -> Result<impl Future<Output = ()>> {
    Ok(async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => println!("\nCtrl+C received, shutting down..."),
            Err(e) => eprintln!("\nCtrl+C handler error, shutting down: {e}"),
        }
    })
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}
