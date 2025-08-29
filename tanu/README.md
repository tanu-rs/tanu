# tanu

[![Crates.io](https://img.shields.io/crates/v/tanu)](https://crates.io/crates/tanu)
[![Documentation](https://docs.rs/tanu/badge.svg)](https://docs.rs/tanu)
[![License](https://img.shields.io/crates/l/tanu)](https://github.com/tanu-rs/tanu/blob/main/LICENSE)

High-performance, async-friendly and ergonomic WebAPI testing framework for Rust.

## Overview

`tanu` is the main crate for the tanu WebAPI testing framework. It provides a complete solution for testing REST APIs with a focus on performance, ergonomics, and type safety.

## Key Features

- **Fast and Lightweight** - Leverages Rust's zero-cost abstractions for minimal overhead
- **Type-Safe and Ergonomic** - Takes advantage of Rust's strong type system to prevent errors at compile time
- **Async/Await Native** - Full support for async operations without boilerplate
- **Concurrent Execution** - Built-in support for running tests concurrently
- **Parameterized Testing** - Test multiple scenarios with different inputs
- **Interactive TUI** - Beautiful terminal UI for test execution and monitoring
- **CLI Interface** - Simple command-line interface for CI/CD integration
- **Allure Integration** - Generate beautiful HTML test reports

## Quick Start

### Installation

Add tanu to your `Cargo.toml`:

```toml
[dependencies]
tanu = "0.9.0"
tokio = { version = "1", features = ["full"] }
eyre = "0.6"
```

### Basic Usage

```rust
use tanu::{check, check_eq, http::Client};

#[tanu::test]
async fn get_user_profile() -> eyre::Result<()> {
    let client = Client::new();

    let response = client
        .get("https://api.example.com/users/123")
        .header("authorization", "Bearer token123")
        .send()
        .await?;

    check!(response.status().is_success(), "Expected successful response");

    let user: serde_json::Value = response.json().await?;
    check_eq!(123, user["id"].as_i64().unwrap());
    check_eq!("John Doe", user["name"].as_str().unwrap());

    Ok(())
}

#[tanu::test(200)]
#[tanu::test(404)]
#[tanu::test(500)]
async fn test_status_codes(expected_status: u16) -> eyre::Result<()> {
    let client = Client::new();
    let response = client
        .get(&format!("https://httpbin.org/status/{expected_status}"))
        .send()
        .await?;

    check_eq!(expected_status, response.status().as_u16());
    Ok(())
}

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = tanu::run();
    let app = tanu::App::new();
    app.run(runner).await
}
```

### Running Tests

Run your tests with:

```bash
cargo run
```

Or use the TUI mode for interactive testing:

```bash
cargo run -- tui
```

## CLI Options

```bash
Usage: your-test-binary [OPTIONS]

Options:
  -c, --config <FILE>     Configuration file path [default: tanu.toml]
  -p, --project <NAME>    Project name to run
  -t, --test <PATTERN>    Test name pattern to run
  tui                     Launch interactive TUI mode
      --parallel <N>      Number of parallel test executions
      --timeout <SECS>    Global timeout for test execution
  -h, --help              Print help information
  -V, --version           Print version information
```

## Configuration

Create a `tanu.toml` file to configure your tests:

```toml
[[projects]]
name = "api-tests"
test_ignore = ["slow_test", "flaky_test"]

[projects.retry]
count = 3
factor = 2.0
jitter = true
delays = ["1s", "2s", "5s"]

[projects.tui.payload.color_theme]
keyword = "blue"
string = "green"
number = "yellow"
boolean = "magenta"
null = "red"
```

## HTTP Client Features

The HTTP client supports:

- **JSON Support** - Automatic JSON serialization/deserialization
- **Form Data** - URL-encoded and multipart form data
- **Headers** - Easy header management
- **Cookies** - Session management with cookie jars
- **Compression** - gzip, deflate, brotli, zstd support
- **Timeouts** - Configurable request timeouts
- **Retries** - Automatic retry with backoff strategies

### Advanced HTTP Examples

```rust
use tanu::{check, check_eq, http::Client};

#[tanu::test]
async fn post_json_data() -> eyre::Result<()> {
    let client = Client::new();
    let payload = serde_json::json!({
        "name": "John Doe",
        "email": "john@example.com"
    });

    let response = client
        .post("https://api.example.com/users")
        .header("content-type", "application/json")
        .json(&payload)
        .send()
        .await?;

    check_eq!(201, response.status().as_u16());
    Ok(())
}

#[tanu::test]
async fn upload_file() -> eyre::Result<()> {
    let client = Client::new();
    
    let form = reqwest::multipart::Form::new()
        .text("name", "document.pdf")
        .file("file", "/path/to/document.pdf")
        .await?;

    let response = client
        .post("https://api.example.com/upload")
        .multipart(form)
        .send()
        .await?;

    check!(response.status().is_success());
    Ok(())
}
```

## Assertion Macros

Tanu provides ergonomic assertion macros:

```rust
use tanu::{check, check_eq, check_ne, check_str_eq};

// Basic boolean assertion
check!(response.status().is_success(), "Request should succeed");

// Equality assertions
check_eq!(200, response.status().as_u16());
check_ne!(404, response.status().as_u16());

// String equality (with better error messages)
check_str_eq!("application/json", response.headers()["content-type"]);
```

## Features

Enable optional features based on your needs:

```toml
[dependencies]
tanu = { version = "0.9.0", features = ["json", "multipart", "cookies"] }
```

- `json` - JSON request/response support
- `multipart` - Multipart form data support
- `cookies` - Cookie jar support for session management

## Architecture

The tanu framework consists of several crates:

- **`tanu`** - Main crate with CLI and application logic
- **`tanu-core`** - Core HTTP client, assertions, and test runner
- **`tanu-derive`** - Procedural macros for test discovery
- **`tanu-tui`** - Interactive terminal UI

## Examples

Check out the [examples directory](https://github.com/tanu-rs/tanu/tree/main/examples) for more comprehensive examples including:

- REST API testing
- Authentication flows
- File uploads
- Performance testing

## Contributing

Contributions are welcome! Please see the [contributing guide](https://github.com/tanu-rs/tanu/blob/main/CONTRIBUTING.md) for details.

## License

Licensed under the Apache License 2.0 - see the [LICENSE](https://github.com/tanu-rs/tanu/blob/main/LICENSE) file for details.