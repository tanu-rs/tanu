# Getting Started

This guide walks you through creating a tanu test project, writing a first test, and running it from the CLI and the TUI.

## Prerequisites

You need Rust and Cargo. If you don't have them yet, follow the instructions on the [official Rust website](https://www.rust-lang.org/learn/get-started).

## Create a project

tanu tests live in an ordinary binary crate. Create one and add `tanu` and `tokio`:

```bash
cargo new example
cd example
cargo add tanu
cargo add tokio --features full
```

## Set up the entry point

Replace the contents of `src/main.rs` with:

```rust
use tanu::eyre;

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await?;
    Ok(())
}
```

`#[tanu::main]` generates the `run()` function, which builds a test runner containing every `#[tanu::test]` function in the crate. `App::run` parses the command line and runs the requested subcommand.

Run the binary without arguments to see the available commands:

```bash
cargo run
```

```text
tanu CLI offers various commands, including listing and executing test cases

Usage: example <COMMAND>

Commands:
  test  Run tests in CLI mode
  tui   Run tests in TUI mode
  ls    List test cases
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

!!! tip
    Arguments after `--` are passed to your test binary rather than to Cargo, so use `cargo run -- test --capture-http` when you pass flags.

## Write your first test

Add a test function annotated with `#[tanu::test]`. Test functions must be `async` and return a `Result`:

```rust
use tanu::{check, eyre, http::Client};

#[tanu::test]
async fn get() -> eyre::Result<()> {
    let http = Client::new();
    let res = http.get("https://httpbin.org/get").send().await?;
    check!(res.status().is_success());
    Ok(())
}
```

The attribute registers the function with tanu's runner at compile time, so there is nothing else to wire up.

!!! note "Supported error types"
    tanu accepts several `Result` types and converts their errors for reporting:

    - **`eyre::Result<()>`** (recommended) – works directly with the `check!` macros and gives colored backtraces.
    - **`anyhow::Result<()>`** – for existing anyhow-based code.
    - **`Result<(), E>`** – with your own error type or a simple `String`.

    The `check!` macros return early with an `eyre::Report`, so they can only be used in tests that return `eyre::Result`. See [Best Practices](best-practices.md#result-type-flexibility) for details.

## Run the tests

```bash
cargo run -- test
```

```text
✓ 1 [default] example::get (412.08ms)

Tests: 1 passed, 0 failed, 1 total
Time: 413.51ms (prep: 180.22µs)
```

Each line shows the result, a sequence number, the project name in brackets, and the full test name (`crate::module::function`). Without a `tanu.toml`, tanu runs every test in a single project named `default`.

To debug a request, print the captured HTTP traffic:

```bash
cargo run -- test --capture-http
```

To browse tests and their requests interactively, launch the TUI:

```bash
cargo run -- tui
```

## Add configuration

Create a `tanu.toml` next to `Cargo.toml` to run the same tests against multiple environments:

```toml
[[projects]]
name = "staging"
base_url = "https://staging.httpbin.org"

[[projects]]
name = "production"
base_url = "https://httpbin.org"
```

Read project values from your tests with `tanu::get_config()`:

```rust
#[tanu::test]
async fn get() -> eyre::Result<()> {
    let base_url = tanu::get_config().get_str("base_url")?.to_string();
    let http = Client::new();
    let res = http.get(format!("{base_url}/get")).send().await?;
    check!(res.status().is_success());
    Ok(())
}
```

Now every test runs once per project. Use `-p` to pick one:

```bash
cargo run -- test -p staging
```

## TLS backends

tanu supports two TLS backends, selected by Cargo feature flags. Only one can be active at a time.

### `native-tls` (default)

Uses the platform's native TLS stack: **OpenSSL** on Linux, **SChannel** on Windows, and **Secure Transport** on macOS. It is enabled by default and needs no configuration.

```toml
[dependencies]
tanu = "0.22"  # native-tls is enabled by default
```

### `rustls-tls`

[rustls](https://github.com/rustls/rustls) is a pure-Rust TLS library with no dependency on OpenSSL or system TLS libraries, which makes cross-compiling and minimal container images easier. tanu ships three variants. Disable default features and enable one:

| Feature flag | Root certificate source | When to use |
|---|---|---|
| `rustls-tls-webpki-roots` | Bundled [Mozilla WebPKI roots](https://github.com/rustls/webpki-roots) | Recommended for most rustls users; behaves identically on all platforms |
| `rustls-tls-native-roots` | System certificate store (same as `native-tls`) | Your environment has custom or corporate CA certificates installed at the OS level |
| `rustls-tls` | None – you supply roots yourself | Advanced use; prefer one of the variants above |

```toml
# rustls with bundled WebPKI roots
[dependencies]
tanu = { version = "0.22", default-features = false, features = ["rustls-tls-webpki-roots"] }
```

```toml
# rustls with the system certificate store
[dependencies]
tanu = { version = "0.22", default-features = false, features = ["rustls-tls-native-roots"] }
```

!!! note
    `native-tls` and the `rustls-tls*` flags are mutually exclusive. Always set `default-features = false` when enabling a rustls variant; otherwise both backends are enabled and the build fails.

## Other feature flags

| Feature | Enables |
|---|---|
| `json` | `RequestBuilder::json` for sending JSON request bodies |
| `cookies` | `Response::cookies` |
| `grpc` | gRPC call capture – see [gRPC Testing](grpc.md) |
| `graphql` | GraphQL request builder – see [GraphQL Testing](graphql.md) |

## Next steps

- [Test Attributes](attribute.md) – parameterized tests and serial execution
- [Assertions](assertion.md) – `check!`, `check_eq!`, `check_ne!`, `check_str_eq!`
- [Configuration](configuration.md) – projects, retries, environment variables, and credential masking
- [Command Line Options](command-line-option.md) – filtering, concurrency, and reporters
- [API reference on docs.rs](https://docs.rs/tanu)
