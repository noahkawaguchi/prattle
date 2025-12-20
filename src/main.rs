fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .from_env_lossy()
        )
        .init();

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(prattle::run_server(&bind_addr))
}
