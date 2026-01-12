set dotenv-load := true

# The host and port to connect to the server at. The key and fallback should match the ones used by
# the server. However, if running the server using this justfile with `.env` present with
# `BIND_ADDR` set, that value will be passed to the server.
bind-addr := env("BIND_ADDR", "127.0.0.1:8000")

# Connect as a client to the server running locally (default recipe)
connect:
    openssl s_client -connect {{bind-addr}} -quiet

# Run the server
serve:
    cargo run --package prattle-server

# Run all tests in the workspace
test:
    cargo test --workspace --all-targets
    rm -f server/server.crt server/server.key
# (Certificate files are removed after each test run to avoid confusion because tests generate them
# in the `server` subdirectory, while running the server generates them in the project root.)
