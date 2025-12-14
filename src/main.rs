use anyhow::Result;
use std::env;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

async fn async_main() -> Result<()> {
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Listening on {bind_addr}");

    loop {
        let (socket, client_addr) = listener.accept().await?;
        println!("Connection from {client_addr}");

        let (reader, mut writer) = socket.into_split();
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        while buf_reader.read_line(&mut line).await? > 0 {
            print!("Received line: {line}");
            writer.write_all(line.as_bytes()).await?;
            line.clear();
        }

        println!("{client_addr} disconnected");
    }
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}
