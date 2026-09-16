# Assertions

tanu's assertion macros look like the standard `assert!` family, but instead of panicking they record the result with the test runner and return an error from the test function. Every check, passing or failing, shows up in reporters and in the TUI.

| Macro | Checks |
|---|---|
| [`check!`](#check) | A boolean condition |
| [`check_eq!`](#check_eq) | Two values are equal |
| [`check_ne!`](#check_ne) | Two values are not equal |
| [`check_str_eq!`](#check_str_eq) | Two strings are equal, with a line-oriented diff |

All macros accept an optional format string and arguments as the last parameters, just like `assert!`.

## How checks work

When a check fails, the macro:

1. Publishes a failed check event to the runner.
2. Returns early from the test function with an `eyre::Report` (via `?`).
3. The runner marks the test as failed and prints the error with a colored diff and the source location.

Because a failing check returns early, the test function must return a `Result` whose error type can be built from an `eyre::Report`. Use `eyre::Result<()>`:

```rust
use tanu::{check, eyre};

#[tanu::test]
async fn my_test() -> eyre::Result<()> {
    check!(1 + 1 == 2);
    Ok(())
}
```

!!! note
    The macros refer to the `tanu` crate by name, so `tanu` must be a direct dependency of the crate that uses them.

## Available macros

### `check!`

Checks that a boolean expression is `true`.

```rust
use tanu::check;

check!(response.status().is_success());
check!(!users.is_empty(), "expected at least one user, got {}", users.len());
```

**Signatures:**

- `check!(condition)`
- `check!(condition, format, args...)`

### `check_eq!`

Checks that two values are equal. Both sides must implement `PartialEq` and `Debug`. On failure, the values are shown as a colored diff produced by [pretty_assertions](https://crates.io/crates/pretty_assertions).

```rust
use tanu::check_eq;

check_eq!(200, response.status().as_u16());
check_eq!(
    Some("application/json"),
    response.headers().get("content-type").and_then(|v| v.to_str().ok()),
);
check_eq!(expected_user, actual_user, "user payload mismatch");
```

**Signatures:**

- `check_eq!(left, right)`
- `check_eq!(left, right, format, args...)`

### `check_ne!`

Checks that two values are not equal.

```rust
use tanu::check_ne;

check_ne!(500, response.status().as_u16(), "server error");
check_ne!(first["uuid"], second["uuid"]);
```

**Signatures:**

- `check_ne!(left, right)`
- `check_ne!(left, right, format, args...)`

### `check_str_eq!`

Checks that two strings are equal. The diff is rendered line by line, which makes it much easier to spot the difference in multi-line bodies such as JSON, HTML, or text files.

```rust
use tanu::check_str_eq;

let body = response.text().await?;
check_str_eq!(include_str!("fixtures/robots.txt"), body);
check_str_eq!(expected_json, body, "response format changed");
```

**Signatures:**

- `check_str_eq!(left, right)`
- `check_str_eq!(left, right, format, args...)`

## Error output

A failed `check_eq!(200, status, "Expected status code 200")` prints:

```text
✘ 3 [default] example::status (298.33µs):
error:
   0: check failed: `(left == right)`: Expected status code 200

      Diff < left / right > :
      <200
      >404

Location:
   src/status.rs:12
```

For `check_str_eq!`, unchanged lines are shown as context and only the changed lines are highlighted:

```text
   0: check failed: `(left == right)`

      Diff < left / right > :
       hello
      <world
      >World
```

## Compared with `assert!`

| | `assert!` family | `check!` family |
|---|---|---|
| **On failure** | Panics | Returns an `eyre::Report` from the test |
| **Output** | Plain text | Colored diff |
| **Reporting** | Only failures, via the panic message | Every check is published to reporters and the TUI |
| **Macros** | `assert!`, `assert_eq!`, `assert_ne!` | `check!`, `check_eq!`, `check_ne!`, `check_str_eq!` |

Panics inside tests are still caught and reported as failures, so `assert!` and `unwrap()` work, but checks give better output.

## Examples

### HTTP response

```rust
use serde::Deserialize;
use tanu::{check, check_eq, eyre, http::Client};

#[derive(Debug, Deserialize)]
struct User {
    id: u64,
    name: String,
}

#[tanu::test]
async fn list_users() -> eyre::Result<()> {
    let client = Client::new();
    let response = client.get("https://api.example.com/users").send().await?;

    check!(response.status().is_success(), "request failed: {}", response.status());
    check_eq!(
        Some("application/json"),
        response.headers().get("content-type").and_then(|v| v.to_str().ok()),
    );

    let users: Vec<User> = response.json().await?;
    check!(!users.is_empty(), "expected a non-empty user list");
    Ok(())
}
```

### JSON fields

```rust
use serde_json::Value;
use tanu::{check_eq, eyre, http::Client};

#[tanu::test]
async fn config_endpoint() -> eyre::Result<()> {
    let client = Client::new();
    let json: Value = client
        .get("https://api.example.com/config")
        .send()
        .await?
        .json()
        .await?;

    check_eq!("v1.0", json["version"]);
    check_eq!("production", json["environment"]);
    Ok(())
}
```

## Best practices

1. **Add a message when the expression isn't self-explanatory.** The stringified expression is always included, so messages should add context, not repeat it.
   ```rust
   check!(response.status().is_success(), "login failed for {username}");
   ```

2. **Prefer the most specific macro.** `check_eq!(200, status)` shows both values; `check!(status == 200)` doesn't.

3. **Use `check_str_eq!` for long text.** A line diff of two JSON documents is far easier to read than two `Debug`-formatted strings.

4. **Deserialize into structs for structural checks.** A `serde` struct validates field presence and types in one step; see [Best Practices](best-practices.md#use-serde-for-type-safe-response-validation).
