set dotenv-load := true

bind-addr := env("BIND_ADDR", "127.0.0.1:8000")

# Connect as a client to the server running locally (default recipe)
connect:
    openssl s_client -connect {{bind-addr}} -quiet
