# Getting Started

To install `tanu` from [crates.io](https://crates.io), you need to have Rust and Cargo installed on your system. If you don't have Rust installed, you can install it by following the instructions on the [official Rust website](https://www.rust-lang.org/learn/get-started).

Once you have Rust and Cargo installed, create an example project by running the following commands in your terminal:

```bash
cargo new example
cd example
```

Next, you can install `tanu` and `tokio` by running the following commands in your terminal:

```bash
cargo add tanu
cargo add tokio --features full
```

## TLS Backends

Tanu supports two TLS backends, controlled by Cargo feature flags. Only one TLS backend can be active at a time.

### `native-tls` (default)

Uses the platform's native TLS stack: **OpenSSL** on Linux, **SChannel** on Windows, and **Secure Transport** on macOS. This is the default and requires no extra configuration.

```toml
[dependencies]
tanu = "0.x"  # native-tls is enabled by default
```

### `rustls-tls`

Tanu also ships three variants of the [rustls](https://github.com/rustls/rustls) TLS backend. Rustls is a pure-Rust TLS library that does not depend on OpenSSL or any system TLS library, making it easier to cross-compile and deploy in minimal environments.

To switch to rustls, disable the default features and enable one of the variants:

| Feature flag | Root certificate source | When to use |
|---|---|---|
| `rustls-tls-webpki-roots` | Bundled [Mozilla WebPKI roots](https://github.com/rustls/webpki-roots) | Recommended for most rustls users; behaviour is identical across all platforms |
| `rustls-tls-native-roots` | System certificate store (same as `native-tls`) | Needed when your environment has custom/corporate CA certificates installed at the OS level |
| `rustls-tls` | None – you must supply roots yourself | Advanced use; prefer one of the variants above |

```toml
# Example: rustls with bundled WebPKI roots
[dependencies]
tanu = { version = "0.x", default-features = false, features = ["rustls-tls-webpki-roots"] }
```

```toml
# Example: rustls with the system native certificate store
[dependencies]
tanu = { version = "0.x", default-features = false, features = ["rustls-tls-native-roots"] }
```

!!! note
    `native-tls` and the `rustls-tls*` flags are mutually exclusive. Always set `default-features = false` when enabling a rustls variant, otherwise both backends will be enabled and the build will fail.

Open `src/main.rs` in your editor, and replace its contents with the following code:

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

This code sets up a basic `tanu` application using `tokio` for asynchronous runtime and `eyre` for error handling.

To run your application, use the following command in your terminal:

```bash
cargo run
```

you will see the output as follows:
```bash
tanu - High-performance and async-friendly WebAPI testing framework for Rust

Usage: tanu-examples <COMMAND>

Commands:
  test  Run tests in CLI mode
  tui   Run tests in TUI mode
  ls    List test cases
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

If you want to run tests, you can use:

```bash
cargo run test
```

If there are no tests defined, you should see the following output:

```bash
No tests have been defined yet.
```

Next, define your test case. As you can see below, the function has the `#[tanu::test]` attribute. This attribute parses the test function and automatically registers it in the tanu's test runner. The test function has to be "async" and return a `Result<T, E>` type.

!!! note "Supported Error Types"
    Tanu supports various Result types for flexible error handling:

    - **`eyre::Result<()>`** (recommended) - Provides colored backtraces and seamless integration with tanu's assertion macros
    - **`anyhow::Result<()>`** - Compatible with existing anyhow-based code
    - **`std::result::Result<(), E>`** - Standard Rust Result type with custom error types or simple errors like `String`

    For the best experience, we recommend using `eyre::Result` as it integrates perfectly with tanu's `check!` macros and provides excellent error reporting. For more details on error handling best practices, see our [Best Practices](best-practices.md#result-type-flexibility) guide.

```rust
#[tanu::test]
async fn get() -> eyre::Result<()> {
    Ok(())
}
```

Now, define the test assertions in the function:

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

Run the tanu test runner again:

```sh
cargo run test
```

This time you should see the test execution in your terminal like this:

```sh
✓ [default] crate::get
```

tanu offers a TUI-based test runner. To run in TUI mode, use the following command:

```sh
cargo run tui
```

Congratulations! You have successfully set up a basic `tanu` application. For more advanced usage and features, please refer to the [official documentation](https://docs.rs/tanu).
