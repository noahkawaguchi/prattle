use anyhow::Result;
use std::{collections::HashSet, env, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{
        Mutex,
        broadcast::{self, Receiver, Sender},
    },
};

const CHANNEL_CAP: usize = 100;

type Users = Arc<Mutex<HashSet<String>>>;

async fn async_main() -> Result<()> {
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Listening on {bind_addr}");

    let (sender, _) = broadcast::channel(CHANNEL_CAP);
    let users = Arc::new(Mutex::new(HashSet::new()));

    loop {
        let (socket, client_addr) = listener.accept().await?;
        println!("New connection from {client_addr}");

        let tx = sender.clone();
        let rx = tx.subscribe();
        let users_clone = Arc::clone(&users);

        tokio::spawn(async move {
            match handle_client(socket, tx, rx, users_clone).await {
                Err(e) => eprintln!("Error handling client {client_addr}: {e}"),
                Ok(()) => println!("Client {client_addr} disconnected"),
            }
        });
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

async fn handle_client(
    socket: TcpStream,
    tx: Sender<String>,
    mut rx: Receiver<String>,
    users: Users,
) -> Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    let username = loop {
        writer.write_all(b"Choose a username: ").await?;
        buf_reader.read_line(&mut line).await?;
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
    };

    tx.send(format!("* {username} joined the server\n"))?;

    let result = client_loop(
        &mut buf_reader,
        &mut writer,
        &tx,
        &mut rx,
        &users,
        &username,
    )
    .await;

    users.lock().await.remove(&username);

    if let Err(e) = tx.send(format!("* {username} left the server\n")) {
        eprintln!("Failed to broadcast that someone left, maybe no one is online: {e}");
    }

    result
}

async fn client_loop(
    buf_reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    tx: &Sender<String>,
    rx: &mut Receiver<String>,
    users: &Users,
    username: &str,
) -> Result<()> {
    let mut line = String::new();

    loop {
        tokio::select! {
            received_val_result = rx.recv() => {
                writer.write_all(received_val_result?.as_bytes()).await?;
            }

            bytes_read_result = buf_reader.read_line(&mut line) => {
                if bytes_read_result? == 0 {
                    break;
                }

                match Command::from(&line) {
                    Command::Empty => {}
                    Command::Quit => {
                        writer.write_all(b"Goodbye for now!\n").await?;
                        break;
                    }
                    Command::Who => {
                        let users_guard = users.lock().await;
                        let list = users_guard.iter().map(String::as_str).collect::<Vec<_>>();
                        let msg = format!("Currently online: {}\n", list.join(", "));
                        drop(users_guard);
                        writer.write_all(msg.as_bytes()).await?;
                    }
                    Command::Action(action) => {
                        tx.send(format!("* {username} {action}\n"))?;
                    }
                    Command::Msg(msg) => {
                        tx.send(format!("{username}: {msg}\n"))?;
                    }
                }

                line.clear();
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}
