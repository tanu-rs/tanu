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
