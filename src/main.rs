use anyhow::Result;
use std::env;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

async fn async_main() -> Result<()> {
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Listening on {bind_addr}");

    loop {
        let (socket, client_addr) = listener.accept().await?;
        println!("New connection from {client_addr}");

        tokio::spawn(async move {
            match handle_client(socket).await {
                Err(e) => eprintln!("Error handling client {client_addr}: {e}"),
                Ok(()) => println!("Client {client_addr} disconnected"),
            }
        });
    }
}

async fn handle_client(socket: TcpStream) -> Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    while buf_reader.read_line(&mut line).await? > 0 {
        print!("Received line from client: {line}");
        writer.write_all(line.as_bytes()).await?;
        line.clear();
    }

    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}
