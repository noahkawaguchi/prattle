use anyhow::Result;
use std::env;
use tokio::net::TcpListener;

async fn async_main() -> Result<()> {
    let bind_addr = env::var("BIND_ADDR")?;
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Listening on {bind_addr}");

    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}
