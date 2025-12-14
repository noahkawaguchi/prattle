use anyhow::Result;
use std::env;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::broadcast::{self, Receiver, Sender},
};

const CHANNEL_CAP: usize = 100;

async fn async_main() -> Result<()> {
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Listening on {bind_addr}");

    let (sender, _) = broadcast::channel::<String>(CHANNEL_CAP);

    loop {
        let (socket, client_addr) = listener.accept().await?;
        println!("New connection from {client_addr}");

        let tx = sender.clone();
        let rx = tx.subscribe();

        tokio::spawn(async move {
            match handle_client(socket, tx, rx).await {
                Err(e) => eprintln!("Error handling client {client_addr}: {e}"),
                Ok(()) => println!("Client {client_addr} disconnected"),
            }
        });
    }
}

async fn handle_client(
    socket: TcpStream,
    tx: Sender<String>,
    mut rx: Receiver<String>,
) -> Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        tokio::select! {
            bytes_read_result = buf_reader.read_line(&mut line) => {
                if bytes_read_result? == 0 {
                    break;
                }

                tx.send(line.clone())?;
                line.clear();
            }

            received_val_result = rx.recv() => {
                writer.write_all(received_val_result?.as_bytes()).await?;
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
