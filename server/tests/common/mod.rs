pub mod test_client;
pub mod test_server;

mod test_tls;

use anyhow::{Context, Result};
use tracing::level_filters::LevelFilter;

/// The log level to use when running tests, unless overridden by the `RUST_LOG` environment
/// variable. Set to "off" to suppress error messages from expected errors.
const TEST_LOG_LEVEL: LevelFilter = LevelFilter::OFF;

/// Replaces `#[tokio::test]`, not inserting `#[allow(clippy::expect_used)]`.
///
/// Based on the "equivalent code" listed in the docs at
/// <https://docs.rs/tokio/latest/tokio/attr.test.html#using-current-thread-runtime>
pub fn tokio_test<F: Future<Output = Result<()>>>(f: F) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to set up Tokio runtime for test")?
        .block_on(f)
}
