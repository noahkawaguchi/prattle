# Prattle

A TLS-encrypted TCP chat server written in Rust, Prattle demonstrates systems programming concepts such as async I/O, concurrent client handling, network protocol design, and graceful resource management.

## Table of Contents

1. [Features](#features)
2. [Tech Stack](#tech-stack)
3. [Architecture](#architecture)
4. [Commands](#commands)
5. [Running the Server](#running-the-server)
6. [Connecting as a Client](#connecting-as-a-client)
7. [Running Tests](#running-tests)
8. [Project Goals](#project-goals)

## Features

- **TLS Encryption**: All client-server communication is encrypted using Rustls
- **Concurrent Client Handling**: Supports multiple simultaneous clients using both shared state and message passing in Tokio's async runtime
- **Command System**: Simple text-based protocol with commands for chatting, actions, and server queries
- **Backpressure Handling**: Recognizes slow clients and warns them when they fall behind
- **Graceful Shutdown**: Cleanly handles server shutdown with proper client notification and connection draining
- **Strict Code Quality and Testing**: Completely forbids `unsafe`, `unwrap`, and `expect` using Clippy and includes a comprehensive test suite

## Tech Stack

- **[Rust](https://github.com/rust-lang/rust)** - Chosen for performance and concurrency safety
- **[Tokio](https://github.com/tokio-rs/tokio)** - Async runtime for handling concurrent client connections
- **[Rustls](https://github.com/rustls/rustls)** - Modern TLS library for secure encryption
- **[Tracing](https://github.com/tokio-rs/tracing)** - Structured logging for observability

## Architecture

Prattle uses a broadcast channel architecture where:

1. The server accepts TLS connections and spawns a task per client
2. Clients select unique usernames upon connecting
3. Messages are broadcast through a `tokio::sync::broadcast` channel
4. Each client task concurrently manages receiving broadcasts, handling user input, and listening for the shutdown signal
5. Graceful shutdown (via a separate broadcast channel) waits for two-way `close_notify` with timeouts, both per client and globally

## Commands

```
/quit             Leave the server
/help             Show the help message
/who              List online users
/action <action>  Broadcast an action, e.g. /action waves
[anything else]   Send a regular message
```

## Running the Server

```bash
cargo run
```

The server binds to `127.0.0.1:8000` by default. Override with the `BIND_ADDR` environment variable:

```bash
BIND_ADDR=0.0.0.0:9000 cargo run
```

## Connecting as a Client

_Note: Prattle uses self-signed certificates for development, so you'll need to accept the certificate when connecting._

Connect using any TLS-capable client like `openssl s_client`, specifying your custom `BIND_ADDR` or the default `127.0.0.1:8000`:

```bash
openssl s_client -connect 127.0.0.1:8000 -quiet
```

Or, using the command runner [just](https://github.com/casey/just), simply use the command `just`. The `BIND_ADDR` environment variable will be automatically read from a `.env` file if present, falling back to the same default as the server:

```bash
just
```

## Running Tests

```bash
cargo test
```

The suite of unit and integration tests includes spawning a real server and simulating multiple concurrent clients to verify:

- Concurrent connection behavior
- Username validation and collision handling
- Command parsing and execution
- Multi-client broadcasting
- Graceful shutdown with edge cases

## Project Goals

This project was built as a learning exercise to gain and demonstrate experience with:

- Lower-level async programming working directly with Tokio
- Networking concepts and protocol design
- TLS/cryptography in practice
- Rust's ownership model in a concurrent programming context
