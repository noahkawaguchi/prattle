use anyhow::{Result, anyhow};
use std::env;
use tracing::{debug, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    init_tracing_with_default(LevelFilter::INFO)?;

    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:8000"));

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(prattle::run_server(
            &bind_addr,
            prattle::shutdown_signal_handler()?,
        ))
}

/// Installs a global tracing subscriber that defaults to `default_level` unless overridden by the
/// `RUST_LOG` environment variable.
///
/// Also checks for the case where `RUST_LOG` is set to something other than "OFF" (case
/// insensitive), but logging is off, printing a warning to stderr if so.
fn init_tracing_with_default(default_level: LevelFilter) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(default_level.into())
                .from_env_lossy(),
        )
        .try_init()
        .map_err(|e| anyhow!("failed to initialize tracing subscriber: {e}"))?;

    // Both `.from_env()` and `.from_env_lossy()` seem to silently disable logging if RUST_LOG is
    // set to a typo/bogus value, so check if the "error" level is disabled but `RUST_LOG` is set to
    // something other than "OFF" (case insensitive).
    if !tracing::enabled!(tracing::Level::ERROR)
        && let Ok(val) = env::var(EnvFilter::DEFAULT_ENV)
        && !val.eq_ignore_ascii_case(&LevelFilter::OFF.to_string())
    {
        eprintln!(
            "Warning: Logging is off but environment variable {} is: {val}",
            EnvFilter::DEFAULT_ENV
        );
    }

    debug!("Current most verbose log level: {}", LevelFilter::current());

    Ok(())
}
