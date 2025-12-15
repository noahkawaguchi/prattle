fn main() -> anyhow::Result<()> {
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(prattle::run_server(&bind_addr))
}
