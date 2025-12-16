# Frequently Asked Questions (FAQ)

---
tags:
  - FAQ
  - Getting Started
  - Configuration
  - HTTP
---

## General Questions

### What is tanu?
Tanu is a high-performance, async-friendly WebAPI testing framework for Rust. It's designed to be fast, type-safe, ergonomic, and easily extensible with full support for concurrency and async operations.

### How is tanu different from using standard Rust test framework with reqwest?
While you can write API tests using `#[test]` with tokio and reqwest, tanu provides:
- Dedicated test discovery and execution system
- Built-in HTTP client with logging
- Ergonomic assertion macros designed for API testing
- Terminal UI for interactive test execution
- Configuration system for multiple environments
- Parameterized test support

### Is tanu production-ready?
Yes, tanu is actively developed and used for testing production APIs. The framework follows semantic versioning and maintains backward compatibility.

## Installation & Setup

### What are the minimum requirements?
- Rust 1.70 or later
- Cargo package manager
- tokio runtime for async support

### How do I install tanu?
Add tanu to your Cargo.toml:
```bash
cargo add tanu
cargo add tokio --features full
```

### Can I use tanu in an existing Rust project?
Yes! Tanu can be added to any Rust project. You can create a separate binary for your tests or integrate them into your existing test suite.

## Writing Tests

### How do I write a basic test?
```rust
use tanu::{check, eyre, http::Client};

#[tanu::test]
async fn my_test() -> eyre::Result<()> {
    let client = Client::new();
    let response = client.get("https://api.example.com").send().await?;
    check!(response.status().is_success());
    Ok(())
}
```

### Can I use parameterized tests?
Yes! Use multiple `#[tanu::test(param)]` attributes:
```rust
#[tanu::test(200)]
#[tanu::test(404)]
#[tanu::test(500)]
async fn test_status_codes(status: u16) -> eyre::Result<()> {
    // Test implementation
    Ok(())
}
```

### How do I handle authentication?
Add headers to your requests:
```rust
let response = client
    .get("https://api.example.com/protected")
    .header("authorization", "Bearer your-token")
    .send()
    .await?;
```

### What assertion macros are available?
- `check!(condition)` - Basic boolean assertion
- `check_eq!(expected, actual)` - Equality assertion
- `check_ne!(expected, actual)` - Non-equality assertion
- `check_str_eq!(expected, actual)` - String equality with better diff output

## HTTP Features

### Does tanu support cookies?
Yes! Enable the cookies feature:
```toml
tanu = { version = "*", features = ["cookies"] }
```

Then use the cookies API:
```rust
let cookies = response.cookies();
for cookie in cookies {
    println!("{}={}", cookie.name(), cookie.value());
}
```

### What HTTP methods are supported?
All standard HTTP methods: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS.

### Can I send JSON data?
Yes, with the json feature enabled:
```rust
let response = client
    .post("https://api.example.com/users")
    .json(&user_data)
    .send()
    .await?;
```

### How do I handle different content types?
Use appropriate headers:
```rust
let response = client
    .post("https://api.example.com/data")
    .header("content-type", "application/xml")
    .body(xml_data)
    .send()
    .await?;
```

## Configuration

### How do I configure different environments?
Create a `tanu.toml` file:
```toml
[[projects]]
name = "staging"
base_url = "https://staging.api.example.com"

[[projects]]
name = "production"
base_url = "https://api.example.com"
```

### Can I ignore specific tests?
Yes, use the `test_ignore` configuration:
```toml
[[projects]]
name = "default"
test_ignore = ["slow_test", "flaky_test"]
```

### How do I configure retry behavior?
Add retry configuration to your project:
```toml
[[projects]]
name = "default"
retry.count = 3
retry.factor = 2.0
retry.jitter = true
```

## Running Tests

### How do I run tests?
```bash
cargo run                    # Run all tests
cargo run test             # Run all tests (explicit)
cargo run test -t pattern  # Run tests matching pattern
```

### Can I run tests in parallel?
Yes, tanu runs tests concurrently by default. You can control concurrency with command-line options.

### How do I use the TUI mode?
```bash
cargo run tui
```

This opens an interactive terminal interface for running and monitoring tests.

## Troubleshooting

### My tests are failing with connection errors
- Check if the API endpoint is accessible
- Verify network connectivity
- Consider timeouts and retry configuration
- Check if authentication is required

### I see a panic: "cannot access a task-local storage value without setting it first"
This usually happens when you spawn background tasks (e.g. `tokio::spawn`, `JoinSet::spawn`) from inside a `#[tanu::test]` and the spawned task calls `tanu::get_config()` or uses tanu assertion macros (`check!`, `check_eq!`, etc.).

Tokio task-local context is not propagated automatically into spawned tasks. Wrap the spawned future with `tanu::scope_current(...)`:

```rust
#[tanu::test]
async fn spawned_task_uses_tanu_apis() -> eyre::Result<()> {
    let handle = tokio::spawn(tanu::scope_current(async move {
        tanu::check!(true);
        let _cfg = tanu::get_config();
        eyre::Ok(())
    }));
    handle.await??;
    Ok(())
}
```

### I'm getting "function not found" errors
Make sure you've added the required features to your Cargo.toml:
```toml
tanu = { version = "*", features = ["json", "cookies"] }
```

### Tests work individually but fail when run together
This might be due to:
- Shared state between tests
- Rate limiting from the API
- Authentication token expiration
- Resource cleanup issues

### How do I debug HTTP requests?
Tanu automatically captures HTTP request/response logs. Use the TUI mode to inspect detailed request information.

## Performance

### How fast is tanu compared to other tools?
Tanu is built in Rust and leverages zero-cost abstractions for minimal overhead. It typically outperforms JavaScript and Python-based testing frameworks.

### Can I control test execution speed?
Yes, through configuration:
- Adjust concurrency levels
- Configure timeouts
- Use retry settings appropriately
- Consider rate limiting for API protection

## Integration

### Can I use tanu in CI/CD pipelines?
Yes! Tanu works well in CI/CD environments. Use the CLI mode for automated testing:
```bash
cargo run test --reporter json > results.json
```

### How do I integrate with existing test suites?
Tanu tests can run alongside standard Rust tests. You can organize them in separate modules or binaries as needed.

### Can I generate test reports?
Yes, tanu supports various output formats including JSON for integration with reporting tools.

## Contributing

### How can I contribute to tanu?
- Report bugs and feature requests on GitHub
- Submit pull requests with improvements
- Write documentation and examples
- Share your experience with the community

### Where can I get help?
- Check this FAQ and documentation
- Search existing GitHub issues
- Create a new issue for bugs or feature requests
- Join community discussions
