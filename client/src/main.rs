use anyhow::Result;
use prattle_client::client_connection::ClientConnection;
use std::{env, time::Duration};

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut conn = ClientConnection::connect(
                &env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000")),
                CONNECTION_TIMEOUT,
            )
            .await?;

            print!("Read one line from the server: {}", conn.read_line().await?);

            Ok(())
        })
}
