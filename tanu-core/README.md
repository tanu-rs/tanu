# tanu-core

[![Crates.io](https://img.shields.io/crates/v/tanu-core)](https://crates.io/crates/tanu-core)
[![Documentation](https://docs.rs/tanu-core/badge.svg)](https://docs.rs/tanu-core)
[![License](https://img.shields.io/crates/l/tanu-core)](https://github.com/tanu-rs/tanu/blob/main/LICENSE)

The core component of the tanu WebAPI testing framework for Rust.

## Overview

`tanu-core` provides the foundational components for building high-performance, async-friendly WebAPI tests in Rust. It contains the essential building blocks that power the tanu testing framework:

- **HTTP Client**: Async HTTP client built on reqwest with ergonomic API
- **Assertion System**: Type-safe assertion macros for test validation
- **Configuration Management**: Flexible configuration system supporting multiple projects
- **Test Runner**: Async test execution engine with concurrency support
- **Error Handling**: Clean error propagation using eyre

## Key Features

### HTTP Client
- Built on reqwest with async/await support
- Automatic JSON serialization/deserialization
- Cookie support (with `cookies` feature)
- Multipart form data support (with `multipart` feature)
- Gzip, deflate, brotli, and zstd compression support

### Assertion System
The assertion system provides ergonomic macros for test validation:

```rust
use tanu_core::{check, check_eq, check_ne, check_str_eq};

// Basic assertions
check!(response.status().is_success(), "Expected successful response");
check_eq!(200, response.status().as_u16());
check_ne!(404, response.status().as_u16());
check_str_eq!("application/json", response.headers()["content-type"]);
```

### Configuration Management
Supports flexible configuration via `tanu.toml` files:

```toml
[[projects]]
name = "api-tests"
test_ignore = ["slow_test"]

[projects.retry]
count = 3
factor = 2.0
jitter = true
delays = ["1s", "2s", "5s"]
```

### Test Discovery
Compile-time test discovery using procedural macros:

```rust
#[tanu::test]
async fn test_api_endpoint() -> eyre::Result<()> {
    // Test implementation
    Ok(())
}

#[tanu::test(1)]
#[tanu::test(2)]
#[tanu::test(3)]
async fn parameterized_test(param: u32) -> eyre::Result<()> {
    // Test with parameter
    Ok(())
}
```

## Usage

Add `tanu-core` to your `Cargo.toml`:

```toml
[dependencies]
tanu-core = "0.10.0"
```

### Basic HTTP Test Example

```rust
use tanu_core::{check, check_eq, http::Client};

#[tanu::test]
async fn test_get_request() -> eyre::Result<()> {
    let client = Client::new();
    
    let response = client
        .get("https://httpbin.org/get")
        .header("user-agent", "tanu-test")
        .send()
        .await?;
    
    check!(response.status().is_success());
    check_eq!(200, response.status().as_u16());
    
    let json: serde_json::Value = response.json().await?;
    check_eq!("tanu-test", json["headers"]["User-Agent"].as_str().unwrap());
    
    Ok(())
}
```

## Features

- `json` - Enables JSON support for HTTP requests (via reqwest)
- `multipart` - Enables multipart form data support
- `cookies` - Enables cookie jar support for maintaining session state

## Architecture

`tanu-core` is designed with modularity and extensibility in mind:

- **`http`** - HTTP client functionality and request/response handling
- **`assertion`** - Test assertion macros and validation utilities
- **`config`** - Configuration parsing and management
- **`runner`** - Test execution engine with async support
- **`error`** - Error types and handling utilities

## Integration

`tanu-core` is typically used through the main `tanu` crate, but can be used directly for custom testing scenarios or integration into other tools.

For complete examples and documentation, see the [main tanu repository](https://github.com/tanu-rs/tanu).