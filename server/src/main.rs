/// Sets up the async runtime and logging, then runs the server.
fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            prattle_server::logger::init_with_default(tracing::level_filters::LevelFilter::INFO)?;

            prattle_server::server::run(
                &std::env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000")),
                prattle_server::tls::create_config()?,
                prattle_server::shutdown_signal::listen()?,
            )
            .await
        })
}
