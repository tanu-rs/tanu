# tanu-derive

[![Crates.io](https://img.shields.io/crates/v/tanu-derive)](https://crates.io/crates/tanu-derive)
[![Documentation](https://docs.rs/tanu-derive/badge.svg)](https://docs.rs/tanu-derive)
[![License](https://img.shields.io/crates/l/tanu-derive)](https://github.com/tanu-rs/tanu/blob/main/LICENSE)

Procedural macros for the tanu WebAPI testing framework.

## Overview

`tanu-derive` provides the essential procedural macros that power the tanu testing framework:

- **`#[tanu::test]`** - Transforms functions into discoverable test cases
- **`#[tanu::main]`** - Generates the main function for test execution

These macros enable compile-time test discovery and registration, allowing the tanu framework to automatically find and execute your tests without runtime reflection.

## Key Features

### Test Discovery
The `#[tanu::test]` macro automatically registers test functions with the tanu test runner:

```rust
#[tanu::test]
async fn simple_test() -> eyre::Result<()> {
    // Test implementation
    Ok(())
}
```

### Parameterized Tests
Support for parameterized tests with multiple parameter sets:

```rust
#[tanu::test(200)]
#[tanu::test(404)]
#[tanu::test(500)]
async fn test_status_codes(status: u16) -> eyre::Result<()> {
    // Test with different status codes
    Ok(())
}
```

### Main Function Generation
The `#[tanu::main]` macro generates the appropriate main function:

```rust
#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await
}
```

## Usage

Add `tanu-derive` to your `Cargo.toml`:

```toml
[dependencies]
tanu-derive = "0.10.0"
```

However, `tanu-derive` is typically used through the main `tanu` crate, which re-exports these macros:

```toml
[dependencies]
tanu = "0.10.0"
```

## Macro Details

### `#[tanu::test]`

The test macro supports several patterns:

**Basic Test:**
```rust
#[tanu::test]
async fn basic_test() -> eyre::Result<()> {
    // Test code
    Ok(())
}
```

**Parameterized Test:**
```rust
#[tanu::test("GET")]
#[tanu::test("POST")]
#[tanu::test("PUT")]
async fn test_http_methods(method: &str) -> eyre::Result<()> {
    // Test with different HTTP methods
    Ok(())
}
```

**Multiple Parameters:**
```rust
#[tanu::test(200, "OK")]
#[tanu::test(404, "Not Found")]
#[tanu::test(500, "Internal Server Error")]
async fn test_responses(status: u16, message: &str) -> eyre::Result<()> {
    // Test with status code and message
    Ok(())
}
```

### `#[tanu::main]`

The main macro generates the entry point for your test suite:

```rust
use tanu::{run, App};

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = run();
    let app = App::new();
    app.run(runner).await
}
```

## Test Function Requirements

Functions annotated with `#[tanu::test]` must:

1. Be `async` functions
2. Return a `Result` type (typically `eyre::Result<()>`)
3. Take parameters matching the macro arguments (for parameterized tests)

## Error Handling

The macro system integrates seamlessly with Rust's error handling:

```rust
#[tanu::test]
async fn test_with_error_handling() -> eyre::Result<()> {
    let response = some_http_request().await?;
    
    if !response.status().is_success() {
        return Err(eyre::eyre!("Request failed with status: {}", response.status()));
    }
    
    Ok(())
}
```

## Compilation

The macros perform compile-time code generation, creating a test registry that can be efficiently queried at runtime. This approach eliminates the need for runtime reflection while maintaining full type safety.

## Integration

`tanu-derive` is designed to work seamlessly with:
- `tanu-core` for HTTP client and assertions
- `tanu-tui` for interactive test execution
- Standard Rust async ecosystem (tokio, futures, etc.)

For complete examples and documentation, see the [main tanu repository](https://github.com/tanu-rs/tanu).