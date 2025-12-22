fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            prattle::logger::init_with_default(tracing::level_filters::LevelFilter::INFO)?;

            prattle::server::run(
                &std::env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000")),
                prattle::tls::create_config()?,
                prattle::shutdown_signal::listen()?,
            )
            .await
        })
}
