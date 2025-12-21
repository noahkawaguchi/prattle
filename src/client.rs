use crate::{
    command::{COMMAND_HELP, Command},
    server::GLOBAL_SHUTDOWN_TIMEOUT,
};
use anyhow::Result;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{
        Mutex,
        broadcast::{Receiver, Sender, error::RecvError},
    },
};
use tracing::{error, info, warn};

/// The time to wait for an individual client to close their connection during graceful shutdown.
const CLIENT_SHUTDOWN_TIMEOUT: Duration =
    GLOBAL_SHUTDOWN_TIMEOUT.saturating_sub(Duration::from_secs(1));

type Users = Arc<Mutex<HashSet<String>>>;

pub async fn handle_client<S>(
    socket: S,
    tx: Sender<String>,
    rx: Receiver<String>,
    shutdown_rx: Receiver<()>,
    users: Users,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, writer) = tokio::io::split(socket);

    let mut handler =
        ClientHandler { reader: BufReader::new(reader), writer, tx, rx, shutdown_rx, users };

    handler.run().await
}

struct ClientHandler<R, W> {
    reader: BufReader<R>,
    writer: W,
    tx: Sender<String>,
    rx: Receiver<String>,
    shutdown_rx: Receiver<()>,
    users: Users,
}

impl<R, W> ClientHandler<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    async fn run(&mut self) -> Result<()> {
        let mut line = String::new();

        let username = loop {
            self.writer.write_all(b"Choose a username: ").await?;
            self.reader.read_line(&mut line).await?;
            let read_username = line.trim().to_string();
            line.clear();

            if read_username.is_empty() {
                self.writer.write_all(b"Username cannot be empty\n").await?;
            } else {
                let mut users_guard = self.users.lock().await;

                if users_guard.contains(&read_username) {
                    drop(users_guard);
                    self.writer.write_all(b"Username taken\n").await?;
                } else {
                    users_guard.insert(read_username.clone());
                    drop(users_guard);
                    break read_username;
                }
            }
        };

        self.writer
            .write_all(
                format!("Hi {username}, welcome to Prattle! (Send /help for help)\n").as_bytes(),
            )
            .await?;

        self.tx.send(format!("* {username} joined the server\n"))?;

        let result = self.command_loop(&username).await;

        self.users.lock().await.remove(&username);

        if let Err(e) = self.tx.send(format!("* {username} left the server\n")) {
            warn!("Failed to broadcast that {username} left: {e}");
        }

        result
    }

    async fn command_loop(&mut self, username: &str) -> Result<()> {
        let mut line = String::new();

        loop {
            tokio::select! {
                received_val_result = self.rx.recv() => {
                    self.writer.write_all(received_val_result?.as_bytes()).await?;
                }

                bytes_read_result = self.reader.read_line(&mut line) => {
                    if bytes_read_result? == 0 {
                        break;
                    }

                    if self.run_command(Command::parse(&line), username).await? {
                        break;
                    }

                    line.clear();
                }

                shutdown_result = self.shutdown_rx.recv() => {
                    self.handle_shutdown(username, shutdown_result).await;
                    break;
                }
            }
        }

        Ok(())
    }

    /// Runs the command or sends the message, returning `Ok(true)` to quit.
    async fn run_command(&mut self, command: Command<'_>, username: &str) -> Result<bool> {
        let should_quit = match command {
            Command::Empty => false,

            Command::Quit => {
                self.writer.write_all(b"Goodbye for now!\n").await?;

                if let Err(e) = self.writer.shutdown().await {
                    warn!("Error shutting down output stream for {username}: {e}");
                }

                true
            }

            Command::Help => {
                self.writer.write_all(COMMAND_HELP).await?;
                false
            }

            Command::Who => {
                let users_guard = self.users.lock().await;
                let list = users_guard.iter().map(String::as_str).collect::<Vec<_>>();
                let msg = format!("Currently online: {}\n", list.join(", "));
                drop(users_guard);
                self.writer.write_all(msg.as_bytes()).await?;
                false
            }

            Command::Action(action) => {
                self.tx.send(format!("* {username} {action}\n"))?;
                false
            }

            Command::Msg(msg) => {
                self.tx.send(format!("{username}: {msg}\n"))?;
                false
            }
        };

        Ok(should_quit)
    }

    /// Sends a shutdown message to the client and waits for them to close the connection, timing
    /// out if they fail to disconnect gracefully. Logs any errors encountered instead of returning
    /// them.
    async fn handle_shutdown(&mut self, username: &str, signal_result: Result<(), RecvError>) {
        if let Err(e) = signal_result {
            error!("Error receiving shutdown signal for {username}: {e}");
        }

        if let Err(e) = self.writer.write_all(b"Server is shutting down\n").await {
            error!("Error sending shutdown message to {username}: {e}");
        }

        // Close the write side
        if let Err(e) = self.writer.shutdown().await {
            warn!("Error shutting down output stream for {username}: {e}");
        }

        let mut discard = Vec::new();

        // Wait for the read side to be closed by the client or time out
        if tokio::time::timeout(
            CLIENT_SHUTDOWN_TIMEOUT,
            self.reader.read_to_end(&mut discard),
        )
        .await
        .is_ok_and(|read_res| read_res.is_ok())
        {
            info!("{username} closed connection gracefully");
        } else {
            warn!(
                "{username} did not close connection gracefully within timeout, forcing disconnect"
            );
        }
    }
}
