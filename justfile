# Both server and client use `BIND_ADDR` if present
set dotenv-load

# Connect to the server as a client (default recipe)
connect:
    cargo run --package prattle-client

# Run the server
serve:
    cargo run --package prattle-server

# Run tests, lints, format checking, and spell checking to match CI
all-checks: (test '--quiet') lint fmt-check spell-check

# Certificate files are removed after each test run to avoid confusion because tests generate them
# in the `server` subdirectory, while running the server generates them in the project root.
[doc('Run all tests in the workspace')]
test *ARGS:
    cargo test --workspace --all-targets {{ ARGS }}
    rm -f server/server.crt server/server.key

# Lint with Clippy, denying warnings
lint:
    cargo clippy --workspace --all-targets -- --deny warnings

# Check formatting
fmt-check:
    cargo fmt --all --check && echo 'Formatting check passed'

# Check spelling with Codebook
spell-check:
    git ls-files -z | xargs -0 codebook-lsp lint
