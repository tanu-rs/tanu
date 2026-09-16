# `#[tanu::test]` Attribute

The `#[tanu::test]` attribute marks a function as a tanu test. Annotated functions are registered with the runner at compile time and executed by `test`, `tui`, and listed by `ls`.

## Usage

```rust
use tanu::{check_eq, eyre};

#[tanu::test]
async fn my_test_function() -> eyre::Result<()> {
    let result = 2 + 2;
    check_eq!(4, result);
    Ok(())
}
```

Requirements:

- The function must be `async`.
- The function must return a `Result<(), E>`. `eyre::Result<()>` is recommended because the `check!` macros return `eyre` errors; `anyhow::Result` and custom error types also work (see [Best Practices](best-practices.md#result-type-flexibility)).
- A test without attribute arguments must not take parameters. Parameterized tests take one parameter per attribute argument.

### Test names

A test's full name is `<module path>::<function name>`, where the module path starts with your crate name. For a crate named `example`:

| Definition | Full name |
|---|---|
| `async fn get()` in `src/main.rs` | `example::get` |
| `async fn get()` in `src/http.rs` | `example::http::get` |

Use the full name with `--tests`, `test_ignore`, and `test_only`. Run `cargo run -- ls` to see the names tanu generated.

## Parameterized Tests

Inspired by the [test-case](https://crates.io/crates/test-case) crate, you can parameterize a test by passing arguments to the attribute. Each attribute generates a separate test case:

```rust
use tanu::{check_eq, eyre};

#[tanu::test(10, 10, 20)]
#[tanu::test(20, 20, 40)]
async fn add(a: u32, b: u32, expected: u32) -> eyre::Result<()> {
    check_eq!(expected, a + b);
    Ok(())
}
```

tanu names each case by joining the stringified arguments with `_` and appending the result to the function name:

```text
example::add::10_10_20
example::add::20_20_40
```

Some arguments can't be turned into a readable name, and some names are simply unclear. In those cases, give each case an explicit name after a `;`:

```rust
use tanu::{check_eq, eyre};

#[tanu::test(10, 10, 20; "ten_plus_ten")]
#[tanu::test(20, 20, 40; "twenty_plus_twenty")]
async fn add(a: u32, b: u32, expected: u32) -> eyre::Result<()> {
    check_eq!(expected, a + b);
    Ok(())
}
```

```text
example::add::ten_plus_ten
example::add::twenty_plus_twenty
```

## Serial Execution

By default, tanu runs tests in parallel. Some tests need to run one at a time, for example tests that:

- Share mutable state (databases, files, environment variables)
- Modify global resources
- Exhaust a limited resource such as ports or rate limits

The `serial` argument prevents such tests from overlapping.

### Basic serial execution

`serial` without a group name puts the test in a default group shared by every other `serial` test in the project:

```rust
#[tanu::test(serial)]
async fn database_setup() -> eyre::Result<()> {
    // Never runs at the same time as another `serial` test
    Ok(())
}

#[tanu::test(serial)]
async fn database_cleanup() -> eyre::Result<()> {
    Ok(())
}
```

!!! note
    `serial` guarantees mutual exclusion, not order. If tests must run in a specific sequence, use [ordered execution](ordered-execution.md).

### Named serial groups

Named groups isolate serial execution. Tests in the same group never overlap, while tests in different groups run in parallel:

```rust
#[tanu::test(serial = "database")]
async fn db_write() -> eyre::Result<()> {
    // Serialized only with other "database" tests
    Ok(())
}

#[tanu::test(serial = "database")]
async fn db_read() -> eyre::Result<()> {
    Ok(())
}

#[tanu::test(serial = "cache")]
async fn cache_write() -> eyre::Result<()> {
    // May run in parallel with the "database" group
    Ok(())
}
```

### Serial with parameters

`serial` can be combined with parameterized tests and may appear anywhere in the argument list:

```rust
// Serial before parameters
#[tanu::test(serial, 200)]
#[tanu::test(serial, 404)]
async fn status_codes(code: u16) -> eyre::Result<()> {
    Ok(())
}

// Serial after parameters
#[tanu::test(1, 2, serial)]
#[tanu::test(3, 4, serial)]
async fn addition(a: i32, b: i32) -> eyre::Result<()> {
    Ok(())
}

// Named group with parameters
#[tanu::test(serial = "api", 200)]
#[tanu::test(serial = "api", 404)]
async fn api_codes(code: u16) -> eyre::Result<()> {
    Ok(())
}
```

### How serial groups behave

- **Project-scoped**: groups are scoped per project, so the same group name in different projects doesn't block.
- **Lock before permit**: a serial test acquires its group lock before taking a slot from the global concurrency limit, so waiting tests don't hold slots other tests could use.
- **Minimal lock scope**: the lock covers test execution only, not setup, teardown, or retry backoff.
- **Everything else stays parallel**: non-serial tests and other groups keep running concurrently.

## Ordered Execution

Apply `#[tanu::test(ordered)]` to a module to run its tests sequentially in source order. See [Ordered Execution](ordered-execution.md).
