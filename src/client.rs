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
        broadcast::{Receiver, Sender},
    },
};
use tracing::{error, info, warn};

/// The time to wait for a client to close their connection before forcefully disconnecting.
const CLIENT_DISCONNECT_TIMEOUT: Duration =
    GLOBAL_SHUTDOWN_TIMEOUT.saturating_sub(Duration::from_secs(1));

/// The placeholder username to use if a client has not yet chosen a username.
const UNKNOWN_USERNAME: &str = "[unknown]";

type Users = Arc<Mutex<HashSet<String>>>;

pub async fn handle_client<S>(
    socket: S,
    tx: Sender<String>,
    rx: Receiver<String>,
    mut shutdown_rx: Receiver<()>,
    users: Users,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (inner_reader, mut writer) = tokio::io::split(socket);
    let mut reader = BufReader::new(inner_reader);

    let mut line = String::new();

    let username = loop {
        tokio::select! {
            shutdown_result = shutdown_rx.recv() => {
                if let Err(e) = shutdown_result {
                    error!("Error receiving shutdown signal during username selection: {e}");
                }

                // Attempt graceful disconnect regardless of the write result, but still report
                // write errors to the main server loop
                let write_res = writer.write_all(b"\nServer is shutting down\n").await;
                graceful_disconnect(&mut reader, &mut writer, UNKNOWN_USERNAME).await;
                return write_res.map_err(Into::into);
            }

            read_result = async {
                writer.write_all(b"Choose a username: ").await?;
                reader.read_line(&mut line).await
            } => {
                read_result?;
                let read_username = line.trim().to_string();
                line.clear();

                if read_username.is_empty() {
                    writer.write_all(b"Username cannot be empty\n").await?;
                } else {
                    let mut users_guard = users.lock().await;

                    if users_guard.contains(&read_username) {
                        drop(users_guard);
                        writer.write_all(b"Username taken\n").await?;
                    } else {
                        users_guard.insert(read_username.clone());
                        drop(users_guard);
                        break read_username;
                    }
                }
            }
        }
    };

    ClientHandler { reader, writer, tx, rx, shutdown_rx, username, users }
        .run()
        .await
}

/// Shuts down the output stream and waits for the client to close the connection, timing out if
/// they fail to disconnect gracefully. Logs any errors encountered instead of returning them.
async fn graceful_disconnect<R, W>(reader: &mut BufReader<R>, writer: &mut W, username: &str)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // Close the write side
    if let Err(e) = writer.shutdown().await {
        error!("Error shutting down output stream for {username}: {e}");
    }

    let mut discard = Vec::new();

    // Wait for the read side to be closed by the client or time out
    if tokio::time::timeout(CLIENT_DISCONNECT_TIMEOUT, reader.read_to_end(&mut discard))
        .await
        .is_ok_and(|read_res| read_res.is_ok())
    {
        info!("{username} closed connection gracefully");
    } else {
        warn!("{username} did not close connection gracefully within timeout, forcing disconnect");
    }
}

struct ClientHandler<R, W> {
    reader: BufReader<R>,
    writer: W,
    tx: Sender<String>,
    rx: Receiver<String>,
    shutdown_rx: Receiver<()>,
    username: String,
    users: Users,
}

impl<R, W> ClientHandler<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    async fn run(&mut self) -> Result<()> {
        self.writer
            .write_all(
                format!(
                    "Hi {}, welcome to Prattle! (Send /help for help)\n",
                    self.username
                )
                .as_bytes(),
            )
            .await?;

        self.tx
            .send(format!("* {} joined the server\n", self.username))?;

        let loop_res = self.command_loop().await;

        self.users.lock().await.remove(&self.username);

        if let Err(e) = self
            .tx
            .send(format!("* {} left the server\n", self.username))
        {
            warn!("Failed to broadcast that {} left: {e}", self.username);
        }

        loop_res
    }

    async fn command_loop(&mut self) -> Result<()> {
        let mut line = String::new();

        loop {
            tokio::select! {
                received_val_result = self.rx.recv() => {
                    self.writer.write_all(received_val_result?.as_bytes()).await?;
                }

                bytes_read_result = self.reader.read_line(&mut line) => {
                    if bytes_read_result? == 0 {
                        warn!("Received EOF from {} without proper disconnection", self.username);
                        break Ok(());
                    }

                    // Run the command, perform graceful disconnect if necessary, then handle the
                    // result of running the command
                    let command = Command::parse(&line);
                    let cmd_res = self.run_command(&command).await;

                    if command == Command::Quit {
                        graceful_disconnect(&mut self.reader, &mut self.writer, &self.username)
                            .await;
                        break cmd_res;
                    }

                    cmd_res?;
                    line.clear();
                }

                shutdown_result = self.shutdown_rx.recv() => {
                    if let Err(e) = shutdown_result {
                        error!("Error receiving shutdown signal for {}: {e}", self.username);
                    }

                    // Attempt graceful disconnect regardless of the write result, but still report
                    // write errors to the main server loop
                    let write_res = self.writer.write_all(b"Server is shutting down\n").await;
                    graceful_disconnect(&mut self.reader, &mut self.writer, &self.username).await;
                    break write_res.map_err(Into::into);
                }
            }
        }
    }

    /// Runs the specified command or sends the specified message.
    async fn run_command(&mut self, command: &Command<'_>) -> Result<()> {
        match command {
            Command::Empty => {}

            // Actually quitting is handled in the main loop
            Command::Quit => self.writer.write_all(b"Goodbye for now!\n").await?,

            Command::Help => self.writer.write_all(COMMAND_HELP).await?,

            Command::Who => {
                let users_guard = self.users.lock().await;
                let list = users_guard.iter().map(String::as_str).collect::<Vec<_>>();
                let msg = format!("Currently online: {}\n", list.join(", "));
                drop(users_guard);
                self.writer.write_all(msg.as_bytes()).await?;
            }

            Command::Action(action) => {
                self.tx.send(format!("* {} {action}\n", self.username))?;
            }

            Command::Msg(msg) => {
                self.tx.send(format!("{}: {msg}\n", self.username))?;
            }
        }

        Ok(())
    }
}
