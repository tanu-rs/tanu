# Frequently Asked Questions

## General

### What is tanu?

tanu is an async-friendly framework for end-to-end testing of HTTP, gRPC, and GraphQL APIs in Rust. Tests are plain `async` functions annotated with `#[tanu::test]`; tanu discovers them at compile time and runs them concurrently from a CLI or an interactive TUI.

### How is tanu different from `#[test]` with tokio and reqwest?

You can write API tests with the standard test harness, but you end up building the surrounding infrastructure yourself. tanu provides:

- Test discovery with parameterized, serial, and ordered tests
- An HTTP client that captures every request and response, with credentials masked
- Assertion macros that report into the runner
- Projects for running the same suite against multiple environments
- Test-level retries, fail-fast, and filters
- A TUI for browsing results and payloads
- Pluggable reporters, including Allure

### Is tanu stable?

tanu is actively developed and used to test real APIs. It is still pre-1.0, so minor releases may contain breaking changes; check the [release notes](https://github.com/tanu-rs/tanu/releases) when upgrading.

## Installation & Setup

### How do I install tanu?

tanu tests live in a binary crate:

```bash
cargo new my-api-tests
cd my-api-tests
cargo add tanu
cargo add tokio --features full
```

Then follow [Getting Started](getting-started.md) to set up `main`.

### Can I add tanu to an existing project?

Yes. The usual approach is a dedicated binary crate in your workspace (for example `api-tests/`) so that test dependencies stay out of your service. Your tests can depend on your service's crates to reuse request and response types.

### Which feature flags do I need?

| Feature | Needed for |
|---|---|
| `json` | `RequestBuilder::json` (sending JSON bodies) |
| `cookies` | `Response::cookies` |
| `grpc` | [gRPC testing](grpc.md) |
| `graphql` | [GraphQL testing](graphql.md) |
| `rustls-tls-webpki-roots` / `rustls-tls-native-roots` | Using rustls instead of native TLS (with `default-features = false`) |

```toml
tanu = { version = "0.22", features = ["json", "cookies"] }
```

If you get a "no method named `json`" error on a request builder, the `json` feature is missing.

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

Yes. Add one `#[tanu::test(...)]` attribute per case:

```rust
#[tanu::test(200)]
#[tanu::test(404)]
#[tanu::test(500)]
async fn status_codes(status: u16) -> eyre::Result<()> {
    Ok(())
}
```

See [Test Attributes](attribute.md#parameterized-tests) for naming rules.

### How do I handle authentication?

Use `bearer_auth`, `basic_auth`, or a header, and keep the secret in an environment variable:

```rust
let token = tanu::get_config().get_str("api_token")?.to_string(); // from TANU_API_TOKEN
let response = client
    .get("https://api.example.com/protected")
    .bearer_auth(token)
    .send()
    .await?;
```

Credentials in headers, query parameters, and JSON/form bodies are masked in HTTP logs by default.

### What assertion macros are available?

| Macro | Checks |
|---|---|
| `check!(cond)` | A boolean condition |
| `check_eq!(left, right)` | Equality, with a colored diff |
| `check_ne!(left, right)` | Inequality |
| `check_str_eq!(left, right)` | String equality, with a line-by-line diff |

All accept an optional format string and arguments. See [Assertions](assertion.md).

### Can tests run in a specific order?

Yes. Use `#[tanu::test(serial)]` or `#[tanu::test(serial = "group")]` to stop tests from overlapping, and `#[tanu::test(ordered)]` on a module to run its tests in source order. See [Ordered Execution](ordered-execution.md).

## HTTP Features

### What HTTP methods are supported?

`GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS`.

### How do I send JSON?

Enable the `json` feature:

```rust
let response = client
    .post("https://api.example.com/users")
    .json(&serde_json::json!({ "name": "Alice" }))
    .send()
    .await?;
```

Reading JSON with `response.json::<T>()` works without the feature.

### How do I send other content types?

Use `form` for URL-encoded bodies, or `body` with an explicit content type:

```rust
let response = client
    .post("https://api.example.com/data")
    .header("content-type", "application/xml")
    .body(xml_data)
    .send()
    .await?;
```

### Does tanu support cookies?

Yes, with the `cookies` feature:

```rust
for cookie in response.cookies() {
    println!("{}={}", cookie.name(), cookie.value());
}
```

### How do I set a request timeout?

```rust
let response = client
    .get("https://api.example.com/slow")
    .timeout(std::time::Duration::from_secs(30))
    .send()
    .await?;
```

## Configuration

### How do I configure different environments?

Define one project per environment in `tanu.toml`:

```toml
[[projects]]
name = "staging"
base_url = "https://staging.api.example.com"

[[projects]]
name = "production"
base_url = "https://api.example.com"
```

Every test runs once per project. Read values with `tanu::get_config().get_str("base_url")`, and select projects with `-p`. See [Configuration](configuration.md).

### Can I skip specific tests in a project?

Yes, with `test_ignore`. Use full test names, including the crate name:

```toml
[[projects]]
name = "production"
test_ignore = ["my_api_tests::users::delete_account"]
```

### Can I run only specific tests in a project?

Yes, with `test_only`. An empty list runs all tests:

```toml
[[projects]]
name = "production"
test_only = ["my_api_tests::health::health_check", "my_api_tests::auth::login"]
```

### How do I configure retries?

Retries re-run a failed test with exponential backoff:

```toml
[[projects]]
name = "default"
retry.count = 3
retry.factor = 2.0
retry.jitter = true
```

See [Retry](configuration.md#retry).

## Running Tests

### How do I run tests?

```bash
cargo run -- test                               # run all tests
cargo run -- test -p staging                    # one project
cargo run -- test -m my_api_tests::users        # one module
cargo run -- test -t my_api_tests::users::login # one test
cargo run -- ls                                 # list test names
```

Module and test filters match full names exactly; they are not patterns. See [Command Line Options](command-line-option.md).

### Can I run tests in parallel?

Tests run in parallel by default, with no limit in CLI mode. Limit concurrency with `-c 4` or `runner.concurrency = 4` in `tanu.toml`.

### How do I use the TUI?

```bash
cargo run -- tui
```

See [TUI](tui.md) for key bindings.

## Troubleshooting

### I see a panic: "cannot access a task-local storage value without setting it first"

This happens when you spawn a task (e.g. `tokio::spawn`, `JoinSet::spawn`) from a `#[tanu::test]` and the spawned task calls `tanu::get_config()` or a `check!` macro.

Tokio task-locals are not propagated into spawned tasks. Wrap the future with `tanu::scope_current(...)`:

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

### `get_int` returns "value not found" for a number in `tanu.toml`

The typed accessors parse string values. Quote the value (`timeout = "5000"`) or read the raw TOML value with `get`. See [User-defined settings](configuration.md#user-defined-settings).

### My test is listed but doesn't run with `-t` or `test_ignore`

Test names include the crate name, e.g. `my_api_tests::users::login` rather than `users::login`. Run `cargo run -- ls` and copy the name from there.

### Tests pass individually but fail together

Common causes:

- Shared state between tests — use [serial groups](attribute.md#serial-execution)
- API rate limiting — lower concurrency with `-c`
- Tests depending on data created by other tests — use [ordered execution](ordered-execution.md) or make them independent
- Expired authentication tokens

### How do I debug HTTP requests?

By default, captured HTTP logs are printed for failed tests. To print them for every test:

```bash
cargo run -- test --capture-http
```

Or set it in `tanu.toml`:

```toml
[runner]
capture_http = "all"
```

Large bodies are truncated at 16KB; raise the limit with `--max-body-size`. Use `--show-sensitive` locally if you need to see masked values. The TUI shows the same information interactively.

## Integration

### Can I use tanu in CI?

Yes. Use the CLI mode; the process exits with a non-zero status when tests fail:

```bash
cargo run -- test --color always --fail-fast
```

### Can I generate test reports?

Yes. Use [tanu-allure](report.md#allure) for Allure reports, or implement the [`Reporter`](report.md#writing-a-custom-reporter) trait for your own format.

## Contributing

### How can I contribute?

- Report bugs and request features in [GitHub issues](https://github.com/tanu-rs/tanu/issues)
- Submit pull requests
- Improve documentation and examples

### Where can I get help?

Search the [existing issues](https://github.com/tanu-rs/tanu/issues) or open a new one.
