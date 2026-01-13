use anyhow::{Context, Result};
use std::{env, io::BufRead, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// The amount of time to wait when connecting to the server.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Sets up the async runtime and calls `async_main`.
fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

/// Connects to the server and writes to/reads from it using stdin/stdout until mutual
/// `close_notify` (initiated by a "/quit" command).
async fn async_main() -> Result<()> {
    let (mut reader, mut writer) = prattle_client::connect(
        &env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000")),
        CONNECTION_TIMEOUT,
    )
    .await?;

    // Channel to send stdin lines from OS thread (unbounded because human input is small and much
    // slower than network writes, MPSC for simplicity given Tokio's API even though it's SPSC)
    let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel();

    // Spawn a native OS thread that blocks reading from stdin. This thread is intentionally not
    // manually joined so that the process can exit immediately after closing the TLS connection
    // rather than waiting for the blocking `read` syscall to complete. Since the only resource
    // this thread holds is stdin, the OS cleans up properly when the process exits.
    std::thread::spawn(move || {
        for line_result in std::io::stdin().lock().lines() {
            match line_result {
                Err(e) => {
                    eprintln!("Error reading line from stdin: {e}");
                    break;
                }

                Ok(line) => {
                    if let Err(e) = stdin_tx.send(line) {
                        eprintln!("Error sending line to stdin channel: {e}");
                        break;
                    }
                }
            }
        }
    });

    // Future that reads from the server and prints to stdout
    let server_to_stdout = async {
        let mut line = String::new();

        loop {
            match reader.read_line(&mut line).await {
                Err(e) => {
                    eprintln!("Error reading line from server: {e}");
                    break;
                }

                Ok(bytes_read) => {
                    // `Ok(0)` means EOF (server closed connection after client sent "/quit")
                    if bytes_read == 0 {
                        break;
                    }

                    // Print to stdout (line already includes newline)
                    print!("{line}");
                }
            }

            line.clear();
        }
    };

    // Future that reads from stdin and writes to the server
    let stdin_to_server = async {
        // Quitting should always come from sending "/quit" to the server and then completing a
        // two-way TLS `close_notify` initiated by the server. Therefore, it is an error/misuse of
        // the CLI for reading from stdin (this future) to finish first.
        loop {
            let line = stdin_rx.recv().await.context("stdin channel closed")?;
            writer.write_all(line.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
    };

    tokio::select! {
        // This future only finishes first under error/misuse conditions
        result = stdin_to_server => result,

        // Normal path: client sent "/quit" -> server sent `close_notify` -> now client sends
        // `close_notify` and exits
        () = server_to_stdout => writer.shutdown().await.map_err(Into::into),
    }
}
