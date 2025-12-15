use crate::Users;
use anyhow::Result;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::broadcast::{Receiver, Sender},
};

pub struct ClientConn {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    tx: Sender<String>,
    rx: Receiver<String>,
    shutdown_rx: Receiver<()>,
    users: Users,
}

impl ClientConn {
    pub fn new(
        socket: TcpStream,
        tx: Sender<String>,
        rx: Receiver<String>,
        shutdown_rx: Receiver<()>,
        users: Users,
    ) -> Self {
        let (inner_reader, writer) = socket.into_split();
        let reader = BufReader::new(inner_reader);
        Self { reader, writer, tx, rx, shutdown_rx, users }
    }

    pub async fn handle(&mut self) -> Result<()> {
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

        self.tx.send(format!("* {username} joined the server\n"))?;

        let result = self.command_loop(&username).await;

        self.users.lock().await.remove(&username);

        if let Err(e) = self.tx.send(format!("* {username} left the server\n")) {
            eprintln!("Failed to broadcast that {username} left: {e}");
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

                    match Command::from(&line) {
                        Command::Empty => {}
                        Command::Quit => {
                            self.writer.write_all(b"Goodbye for now!\n").await?;
                            break;
                        }
                        Command::Who => {
                            let users_guard = self.users.lock().await;
                            let list = users_guard.iter().map(String::as_str).collect::<Vec<_>>();
                            let msg = format!("Currently online: {}\n", list.join(", "));
                            drop(users_guard);
                            self.writer.write_all(msg.as_bytes()).await?;
                        }
                        Command::Action(action) => {
                            self.tx.send(format!("* {username} {action}\n"))?;
                        }
                        Command::Msg(msg) => {
                            self.tx.send(format!("{username}: {msg}\n"))?;
                        }
                    }

                    line.clear();
                }

                shutdown_result = self.shutdown_rx.recv() => {
                    if let Err(e) = shutdown_result {
                        eprintln!("Error receiving shutdown signal for {username}: {e}");
                    }

                    self.writer.write_all(b"Server is shutting down\n").await?;
                    break;
                }
            }
        }

        Ok(())
    }
}

enum Command<'a> {
    Empty,
    Quit,
    Who,
    Action(&'a str),
    Msg(&'a str),
}

impl<'a> Command<'a> {
    fn from(input: &'a str) -> Self {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            Self::Empty
        } else if trimmed == "/quit" {
            Self::Quit
        } else if trimmed == "/who" {
            Self::Who
        } else if let Some(action) = trimmed.strip_prefix("/action ") {
            Self::Action(action)
        } else {
            Self::Msg(trimmed)
        }
    }
}
