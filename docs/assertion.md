# Assertions

Tanu provides a comprehensive assertion system designed specifically for WebAPI testing. The assertion macros follow similar signatures to Rust's standard `assert!` macros but are tailored for testing environments with enhanced error reporting and colored output.

## Key Features

- **Error-based instead of panic-based**: Returns `eyre::Result` instead of panicking
- **Colored output**: Uses `pretty_assertions` for beautiful diff visualization
- **Event publishing**: Publishes test results to the tanu runner for reporting
- **Async-friendly**: Works seamlessly with async test functions

## Available Macros

### `check!`

Basic assertion macro that validates a boolean condition.

```rust
use tanu::check;

#[tanu::test]
async fn basic_check() -> eyre::Result<()> {
    check!(true);
    check!(2 + 2 == 4);
    check!(response.status().is_success(), "Expected successful response");
    Ok(())
}
```

**Signatures:**
- `check!(condition)` - Simple boolean check
- `check!(condition, message, args...)` - With custom error message

### `check_eq!`

Asserts that two values are equal, with pretty-printed diff output for mismatches.

```rust
use tanu::check_eq;

#[tanu::test]
async fn equality_check() -> eyre::Result<()> {
    check_eq!(200, response.status().as_u16());
    check_eq!("application/json", response.headers().get("content-type"));
    check_eq!(expected_payload, actual_payload, "Response payload mismatch");
    Ok(())
}
```

**Signatures:**
- `check_eq!(left, right)` - Basic equality check
- `check_eq!(left, right, message, args...)` - With custom error message

### `check_ne!`

Asserts that two values are not equal.

```rust
use tanu::check_ne;

#[tanu::test]
async fn inequality_check() -> eyre::Result<()> {
    check_ne!(404, response.status().as_u16());
    check_ne!("", response.body());
    Ok(())
}
```

**Signatures:**
- `check_ne!(left, right)` - Basic inequality check
- `check_ne!(left, right, message, args...)` - With custom error message

### `check_str_eq!`

Specialized string equality assertion with enhanced string comparison visualization.

```rust
use tanu::check_str_eq;

#[tanu::test]
async fn string_check() -> eyre::Result<()> {
    check_str_eq!("expected", actual_string);
    check_str_eq!(expected_json, response.text().await?, "JSON response mismatch");
    Ok(())
}
```

**Signatures:**
- `check_str_eq!(left, right)` - Basic string equality check
- `check_str_eq!(left, right, message, args...)` - With custom error message

## Key Differences from Standard `assert!`

| Feature | Standard `assert!` | Tanu `check!` |
|---------|-------------------|---------------|
| **Error handling** | Panics on failure | Returns `eyre::Result` |
| **Output format** | Basic text | Colored with `pretty_assertions` |
| **Integration** | Standalone | Publishes events to tanu runner |
| **Async support** | Limited | Full async/await support |
| **Macro naming** | `assert!`, `assert_eq!`, `assert_ne!` | `check!`, `check_eq!`, `check_ne!`, `check_str_eq!` |

## Error Handling

All assertion macros return `eyre::Result` types, making them compatible with async test functions that return `Result` types. When an assertion fails:

1. An error event is published to the tanu runner
2. An `eyre::Report` is generated with detailed context
3. The error is propagated up the call stack
4. Tanu displays colored backtraces and error information

## Examples

### HTTP Response Testing

```rust
use tanu::{check, check_eq, check_ne, eyre, http::Client};

#[tanu::test]
async fn api_test() -> eyre::Result<()> {
    let client = Client::new();
    let response = client
        .get("https://api.example.com/users")
        .send()
        .await?;

    // Check status code
    check!(response.status().is_success(), "API request failed");
    check_eq!(200, response.status().as_u16());

    // Check headers
    check_ne!("", response.headers().get("content-type").unwrap());
    
    // Check response body
    let users: Vec<User> = response.json().await?;
    check!(!users.is_empty(), "Expected non-empty user list");
    
    Ok(())
}
```

### JSON Response Validation

```rust
use tanu::{check_eq, check_str_eq, eyre, http::Client};
use serde_json::Value;

#[tanu::test]
async fn json_validation() -> eyre::Result<()> {
    let client = Client::new();
    let response = client
        .get("https://api.example.com/config")
        .send()
        .await?;

    let json: Value = response.json().await?;
    
    check_eq!("v1.0", json["version"].as_str().unwrap());
    check_str_eq!("production", json["environment"].as_str().unwrap());
    
    Ok(())
}
```

## Best Practices

1. **Use descriptive messages**: Always provide context for assertion failures
   ```rust
   check!(response.status().is_success(), "Failed to authenticate user");
   ```

2. **Choose the right assertion**: Use `check_str_eq!` for string comparisons to get better diff output
   ```rust
   // Good
   check_str_eq!(expected_json, actual_json);
   
   // Less helpful output
   check_eq!(expected_json, actual_json);
   ```

3. **Combine with HTTP utilities**: Leverage tanu's HTTP client for comprehensive API testing
   ```rust
   let response = client.post("/api/login")
       .json(&credentials)
       .send()
       .await?;
   
   check!(response.status().is_success(), "Login failed");
   ```

4. **Handle async properly**: All assertions work seamlessly in async contexts
   ```rust
   #[tanu::test]
   async fn async_test() -> eyre::Result<()> {
       let result = some_async_operation().await?;
       check!(result.is_valid());
       Ok(())
   }
   ```

## Error Output

When assertions fail, tanu provides rich error information:

```
check failed: `(left == right)`: Expected status code 200

   left: 404
  right: 200

Error: check failed: `(left == right)`: Expected status code 200
```

For string comparisons, you get detailed diff output highlighting the differences between expected and actual values.