# Both server and client use `BIND_ADDR` if present
set dotenv-load := true

# Connect to the server as a client (default recipe)
connect:
    cargo run --package prattle-client

# Run the server
serve:
    cargo run --package prattle-server

# Run all tests in the workspace
test *ARGS:
    cargo test --workspace --all-targets {{ ARGS }}
    rm -f server/server.crt server/server.key
# (Certificate files are removed after each test run to avoid confusion because tests generate them
# in the `server` subdirectory, while running the server generates them in the project root.)
