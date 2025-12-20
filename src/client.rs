use crate::command::{COMMAND_HELP, Command};
use anyhow::Result;
use std::{collections::HashSet, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{
        Mutex,
        broadcast::{Receiver, Sender},
    },
};
use tracing::{error, warn};

type Users = Arc<Mutex<HashSet<String>>>;

pub async fn handle_client(
    socket: TcpStream,
    tx: Sender<String>,
    rx: Receiver<String>,
    shutdown_rx: Receiver<()>,
    users: Users,
) -> Result<()> {
    let (reader, writer) = socket.into_split();

    let mut handler =
        ClientHandler { reader: BufReader::new(reader), writer, tx, rx, shutdown_rx, users };

    handler.run().await
}

struct ClientHandler {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    tx: Sender<String>,
    rx: Receiver<String>,
    shutdown_rx: Receiver<()>,
    users: Users,
}

impl ClientHandler {
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
                    if let Err(e) = shutdown_result {
                        error!("Error receiving shutdown signal for {username}: {e}");
                    }

                    self.writer.write_all(b"Server is shutting down\n").await?;
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
}
