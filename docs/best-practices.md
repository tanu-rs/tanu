# Best Practices

---
tags:
  - Testing
  - HTTP
  - Configuration
---

This guide covers best practices for writing effective API tests with tanu, based on real-world experience and established patterns.

## Test Organization

### Use Descriptive Test Names
Choose test names that clearly describe what is being tested:

```rust
// Good
#[tanu::test]
async fn get_user_returns_valid_profile_data() -> eyre::Result<()> { ... }

#[tanu::test]
async fn create_user_with_invalid_email_returns_400() -> eyre::Result<()> { ... }

// Less clear
#[tanu::test]
async fn test_user() -> eyre::Result<()> { ... }

#[tanu::test]
async fn user_test_2() -> eyre::Result<()> { ... }
```

### Group Related Tests in Modules
Organize tests by feature or endpoint:

```rust
// src/tests/user.rs
mod user {
    #[tanu::test]
    async fn create_user_success() -> eyre::Result<()> { ... }
    
    #[tanu::test]
    async fn get_user_profile() -> eyre::Result<()> { ... }
    
    #[tanu::test]
    async fn update_user_profile() -> eyre::Result<()> { ... }
}

// src/tests/auth.rs
mod auth {
    #[tanu::test]
    async fn login_with_valid_credentials() -> eyre::Result<()> { ... }
    
    #[tanu::test]
    async fn login_with_invalid_credentials() -> eyre::Result<()> { ... }
}
```

## HTTP Best Practices

### Check Response Headers with Original Casing
When validating response headers, use the casing returned by the server:

```rust
let response: HeadersResponse = res.json().await?;

// Server returns headers with original casing
check!(response.headers.contains_key("Content-Type"));
check!(response.headers.contains_key("X-Custom-Header"));
```

### Handle Errors Gracefully
Always handle potential HTTP errors:

```rust
#[tanu::test]
async fn robust_api_test() -> eyre::Result<()> {
    let client = Client::new();
    
    let response = client
        .get("https://api.example.com/users/123")
        .send()
        .await?;
    
    // Check status before processing response
    check!(response.status().is_success(), 
           "Expected successful response, got: {}", response.status());
    
    // Handle potential JSON parsing errors
    let user: serde_json::Value = response.json().await
        .map_err(|e| eyre::eyre!("Failed to parse JSON response: {}", e))?;
    
    Ok(())
}
```

## Parameterized Testing

### Use Parameterized Tests for Similar Scenarios
Instead of duplicating test logic, use parameterized tests:

```rust
// Good - Single test function with multiple parameters
#[tanu::test(200)]
#[tanu::test(404)]
#[tanu::test(500)]
async fn test_status_endpoints(status_code: u16) -> eyre::Result<()> {
    let client = Client::new();
    let response = client
        .get(&format!("https://httpbin.org/status/{status_code}"))
        .send()
        .await?;
    
    check_eq!(status_code, response.status().as_u16());
    Ok(())
}

// Less efficient - Separate functions for each test case
#[tanu::test]
async fn test_200_status() -> eyre::Result<()> { ... }

#[tanu::test]
async fn test_404_status() -> eyre::Result<()> { ... }

#[tanu::test]
async fn test_500_status() -> eyre::Result<()> { ... }
```

### Choose Meaningful Parameter Values
Select parameter values that represent real-world scenarios:

```rust
// Good - Realistic delay values
#[tanu::test(1)]
#[tanu::test(2)]
#[tanu::test(5)]
async fn test_api_timeout_handling(delay_seconds: u64) -> eyre::Result<()> { ... }

// Good - Common HTTP status codes
#[tanu::test(400)]  // Bad Request
#[tanu::test(401)]  // Unauthorized  
#[tanu::test(403)]  // Forbidden
#[tanu::test(404)]  // Not Found
async fn test_error_responses(status_code: u16) -> eyre::Result<()> { ... }
```

## Assertions

### Use Specific Assertion Macros
Choose the most appropriate assertion macro for better error messages:

```rust
// Good - Specific assertions
check_eq!(expected_id, user["id"].as_i64().unwrap());
check_ne!(0, response.headers().len());
check!(response.status().is_success());

// Less informative
check!(user["id"].as_i64().unwrap() == expected_id);
check!(response.headers().len() > 0);
check!(response.status().as_u16() >= 200 && response.status().as_u16() < 300);
```

### Provide Descriptive Error Messages
Add context to your assertions:

```rust
check!(response.status().is_success(), 
       "API should return success status, got: {} - {}", 
       response.status(), 
       response.text().await?);

check_eq!(expected_name, user["name"].as_str().unwrap(),
          "User name should match expected value");
```

### Validate Response Structure
Don't just check status codes - validate the actual response data:

```rust
#[tanu::test]
async fn get_user_returns_complete_profile() -> eyre::Result<()> {
    let response = client.get("/users/123").send().await?;
    check!(response.status().is_success());
    
    let user: serde_json::Value = response.json().await?;
    
    // Validate required fields exist
    check!(user["id"].is_number(), "User ID should be a number");
    check!(user["name"].is_string(), "User name should be a string");
    check!(user["email"].is_string(), "User email should be a string");
    check!(user["created_at"].is_string(), "Created date should be present");
    
    // Validate field values
    check_eq!(123, user["id"].as_i64().unwrap());
    check!(!user["name"].as_str().unwrap().is_empty(), "Name should not be empty");
    
    Ok(())
}
```

### Use Serde for Type-Safe Response Validation
Instead of manually parsing JSON, define response structures with serde for better type safety and automatic validation:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct User {
    id: i64,
    name: String,
    email: String,
    verified: bool,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct CreateUserResponse {
    user: User,
    message: String,
}

#[tanu::test]
async fn create_user_with_serde_validation() -> eyre::Result<()> {
    let user_data = serde_json::json!({
        "name": "John Doe",
        "email": "john@example.com"
    });
    
    let response = client
        .post("/users")
        .json(&user_data)
        .send()
        .await?;
    
    check!(response.status().is_success());
    
    // Serde automatically validates the response structure
    let create_response: CreateUserResponse = response.json().await
        .map_err(|e| eyre::eyre!("Failed to parse response: {}", e))?;
    
    // Type-safe field access
    check_eq!("John Doe", create_response.user.name);
    check_eq!("john@example.com", create_response.user.email);
    check!(!create_response.user.verified); // New users start unverified
    check!(!create_response.message.is_empty());
    
    Ok(())
}
```

**Benefits of using serde:**
- **Compile-time safety**: Field names and types are checked at compile time
- **Automatic validation**: Serde will fail if required fields are missing or have wrong types
- **Better error messages**: Clear errors when deserialization fails
- **Documentation**: Struct definitions serve as API documentation
- **Refactoring safety**: Changes to field names are caught by the compiler

**Optional fields and error handling:**
```rust
#[derive(Debug, Deserialize)]
struct UserProfile {
    id: i64,
    name: String,
    email: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(rename = "created_at")]
    created_date: String,
}

#[tanu::test]
async fn handle_optional_fields_with_serde() -> eyre::Result<()> {
    let response = client.get("/users/123").send().await?;
    check!(response.status().is_success());
    
    let user: UserProfile = response.json().await?;
    
    // Required fields are guaranteed to exist
    check_eq!(123, user.id);
    check!(!user.name.is_empty());
    
    // Optional fields can be safely checked
    if let Some(avatar) = user.avatar_url {
        check!(avatar.starts_with("https://"));
    }
    
    Ok(())
}
```

## Configuration Management

### Use Environment-Specific Configurations
Create separate configurations for different environments:

```toml
# tanu.toml
[[projects]]
name = "local"
base_url = "http://localhost:8080"
timeout = 5000

[[projects]]
name = "staging"
base_url = "https://staging.api.example.com"
timeout = 10000
retry.count = 2

[[projects]]
name = "production" 
base_url = "https://api.example.com"
timeout = 15000
retry.count = 3
retry.factor = 2.0
```

### Ignore Flaky or Slow Tests Appropriately
Use test_ignore for tests that shouldn't run in certain environments:

```toml
[[projects]]
name = "ci"
test_ignore = [
    "slow_integration_test",
    "external_dependency_test",
    "load_test"
]
```

## Performance Considerations

### Minimize Test Dependencies
Keep tests isolated and avoid dependencies between test cases:

```rust
// Good - Each test is independent
#[tanu::test]
async fn create_user_test() -> eyre::Result<()> {
    // Create test data
    // Run test
    // Clean up (if needed)
    Ok(())
}

// Avoid - Tests depending on each other
#[tanu::test]
async fn create_user_first() -> eyre::Result<()> { ... }

#[tanu::test]
async fn update_user_created_above() -> eyre::Result<()> {
    // This test depends on the previous test
    Ok(())
}
```

### Use Serial Execution When Needed

By default, Tanu runs tests in parallel for maximum performance. However, some tests need to run sequentially. Use the `serial` attribute judiciously:

**When to use serial tests:**

1. **Shared mutable state**: Tests that modify shared resources (databases, files, environment variables)
2. **Global state**: Tests that rely on or modify global state
3. **Resource constraints**: Tests that exhaust limited resources (ports, file handles)
4. **External dependencies**: Tests interacting with external systems that don't support concurrency

```rust
// Good - Serial for database tests that share state
#[tanu::test(serial = "database")]
async fn test_database_migration() -> eyre::Result<()> {
    // Modifies shared database schema
    Ok(())
}

#[tanu::test(serial = "database")]
async fn test_database_seeding() -> eyre::Result<()> {
    // Depends on clean database state
    Ok(())
}

// Good - Parallel for independent API tests
#[tanu::test]
async fn test_get_user_endpoint() -> eyre::Result<()> {
    // Read-only, no shared state
    Ok(())
}
```

**Best practices for serial tests:**

- **Use named groups**: Organize serial tests into logical groups (`serial = "database"`, `serial = "cache"`) so different groups can run in parallel
- **Keep them minimal**: Write as few serial tests as possible - they reduce parallelism
- **Isolate state when possible**: Consider using test fixtures or containers to avoid needing serial execution
- **Document why**: Add comments explaining why a test needs serial execution

```rust
// Named groups allow different concerns to run in parallel
#[tanu::test(serial = "database")]
async fn db_test_1() -> eyre::Result<()> {
    // Database tests run sequentially within this group
    Ok(())
}

#[tanu::test(serial = "filesystem")]
async fn file_test_1() -> eyre::Result<()> {
    // Filesystem tests run in parallel with database group
    Ok(())
}

#[tanu::test]
async fn api_test_1() -> eyre::Result<()> {
    // Regular tests run in parallel with everything
    Ok(())
}
```

**Avoid unnecessary serial execution:**

```rust
// Bad - Unnecessarily serial
#[tanu::test(serial)]
async fn test_pure_computation() -> eyre::Result<()> {
    let result = 2 + 2;
    check_eq!(result, 4);
    Ok(())
}

// Good - Parallel by default
#[tanu::test]
async fn test_pure_computation() -> eyre::Result<()> {
    let result = 2 + 2;
    check_eq!(result, 4);
    Ok(())
}
```

**Serial with parameterized tests:**

```rust
// Serial tests can be parameterized too
#[tanu::test(serial = "api", 200)]
#[tanu::test(serial = "api", 201)]
#[tanu::test(serial = "api", 204)]
async fn test_api_status_codes(status: u16) -> eyre::Result<()> {
    // All parameterized tests run serially within the "api" group
    Ok(())
}
```

### Spawning Tokio Tasks (`scope_current`)

Tanu stores per-test context (project/test metadata used by `check!`/`check_eq!` and `get_config()`) in Tokio task-local storage. Tokio task-locals are **not automatically propagated** into tasks created with `tokio::spawn` or `JoinSet::spawn`.

If a spawned task calls `tanu::get_config()` or uses tanu assertion macros, it can panic with:

```
cannot access a task-local storage value without setting it first
```

Wrap spawned futures with `tanu::scope_current(...)` to propagate the current tanu context:

```rust
use tanu::{check, eyre};

#[tanu::test]
async fn concurrent_work_with_spawn() -> eyre::Result<()> {
    let handle = tokio::spawn(tanu::scope_current(async move {
        check!(true);
        let _cfg = tanu::get_config();
        eyre::Ok(())
    }));

    handle.await??;
    Ok(())
}
```

### Use Appropriate Timeouts
Configure timeouts based on expected response times:

```rust
let response = client
    .get("https://api.example.com/slow-endpoint")
    .timeout(Duration::from_secs(30))  // Adjust based on endpoint
    .send()
    .await?;
```

### Batch Related Assertions
Group related assertions together to minimize API calls:

```rust
#[tanu::test]
async fn validate_user_profile_completely() -> eyre::Result<()> {
    let response = client.get("/users/123").send().await?;
    let user: serde_json::Value = response.json().await?;
    
    // Multiple assertions on the same response
    check_eq!(123, user["id"].as_i64().unwrap());
    check_eq!("John Doe", user["name"].as_str().unwrap());
    check_eq!("john@example.com", user["email"].as_str().unwrap());
    check!(user["verified"].as_bool().unwrap());
    
    Ok(())
}
```

## Security Best Practices

### Don't Hardcode Sensitive Data
Use environment variables or configuration for sensitive information:

```rust
// Good
let api_key = std::env::var("API_KEY")
    .map_err(|_| eyre::eyre!("API_KEY environment variable not set"))?;

let response = client
    .get("https://api.example.com/protected")
    .header("authorization", format!("Bearer {}", api_key))
    .send()
    .await?;

// Avoid
let response = client
    .get("https://api.example.com/protected")
    .header("authorization", "Bearer sk-1234567890abcdef")  // Hardcoded!
    .send()
    .await?;
```

!!! tip "Automatic HTTP Log Masking"
    Tanu automatically masks sensitive data (API keys, tokens, passwords) in HTTP logs when using `--capture-http`. See the [Automatic Sensitive Data Masking](#automatic-sensitive-data-masking) section for details.

### Validate SSL Certificates
Ensure your tests validate SSL certificates in production environments (this is the default behavior).

### Use HTTPS in Production Tests
Always use HTTPS endpoints when testing production or staging environments.

### Automatic Sensitive Data Masking

Tanu automatically masks sensitive data in HTTP logs when using the `--capture-http` flag to prevent accidental credential leakage. This feature helps you debug HTTP requests safely without exposing API keys, tokens, or passwords in your terminal output or CI/CD logs.

**What Gets Masked:**

Sensitive query parameters (case-insensitive):
- `api_key`
- `apikey`
- `access_token`
- `token`
- `secret`
- `password`
- `key`
- `auth`

Sensitive headers (case-insensitive):
- `authorization`
- `x-api-key`
- `x-auth-token`
- `cookie`

**How It Works:**

```bash
# Run tests with HTTP logging - sensitive data will be masked
cargo run -- test --capture-http

# Example output (masked):
# => GET https://api.example.com/users?api_key=*****&user=alice
#   > request:
#     > headers:
#        > authorization: *****
#        > content-type: application/json
```

The actual HTTP requests sent to the server contain the real values - only the logs are masked. This means your tests work correctly while keeping your logs secure.

**Show Sensitive Data for Debugging:**

During local development or debugging, you may want to see the actual values:

```bash
# Show sensitive data in logs (use with caution)
cargo run -- test --capture-http --show-sensitive

# Example output (unmasked):
# => GET https://api.example.com/users?api_key=sk-1234567890&user=alice
#   > request:
#     > headers:
#        > authorization: Bearer my-secret-token
#        > content-type: application/json
```

**Best Practices:**

1. **Never use `--show-sensitive` in CI/CD**: Always use the default masked mode in continuous integration environments to prevent credential leaks in build logs.

2. **Review logs before sharing**: Even with masking enabled, review captured HTTP logs before sharing them publicly to ensure no sensitive data is exposed.

3. **Use environment variables**: Store credentials in environment variables rather than hardcoding them:

```rust
#[tanu::test]
async fn api_call_with_credentials() -> eyre::Result<()> {
    let api_key = std::env::var("API_KEY")
        .map_err(|_| eyre::eyre!("API_KEY not set"))?;

    let response = client
        .get("https://api.example.com/protected")
        .query(&[("api_key", api_key)])
        .send()
        .await?;

    check!(response.status().is_success());
    Ok(())
}
```

4. **URL encoding is preserved**: The masking feature preserves URL encoding in query parameters, so `name=john%20doe` remains properly encoded even after masking nearby sensitive params.

5. **Test your integrations safely**: With automatic masking, you can capture HTTP logs even when testing against real APIs with actual credentials, making debugging production issues much safer.

**Example Test:**

```rust
use tanu::{check, check_eq, eyre, http::Client};

#[tanu::test]
async fn test_authenticated_api_call() -> eyre::Result<()> {
    let api_key = std::env::var("API_KEY")?;

    let client = Client::new();
    let response = client
        .get("https://api.example.com/data")
        .header("x-api-key", api_key)
        .query(&[
            ("access_token", "secret_token_123"),
            ("user_id", "alice"),
        ])
        .send()
        .await?;

    check!(response.status().is_success());

    // When run with --capture-http, the logs will show:
    // => GET https://api.example.com/data?access_token=*****&user_id=alice
    //   > request:
    //     > headers:
    //        > x-api-key: *****
    //        > content-type: application/json

    Ok(())
}
```

This automatic masking feature ensures you can debug HTTP requests safely without compromising security, making it ideal for both local development and CI/CD environments.

## Error Handling

### Use Meaningful Error Messages
Provide context when tests fail:

```rust
#[tanu::test]
async fn comprehensive_error_handling() -> eyre::Result<()> {
    let response = client
        .post("https://api.example.com/users")
        .json(&user_data)
        .send()
        .await
        .map_err(|e| eyre::eyre!("Failed to send request to create user: {}", e))?;
    
    if !response.status().is_success() {
        let error_body = response.text().await?;
        return Err(eyre::eyre!(
            "User creation failed with status {}: {}", 
            response.status(), 
            error_body
        ));
    }
    
    Ok(())
}
```

### Handle Rate Limiting
Be respectful of API rate limits:

```rust
use tokio::time::{sleep, Duration};

#[tanu::test]
async fn rate_limited_test() -> eyre::Result<()> {
    for i in 0..10 {
        let response = client.get("/api/endpoint").send().await?;
        
        if response.status().as_u16() == 429 {
            // Rate limited, wait before retrying
            sleep(Duration::from_secs(1)).await;
            continue;
        }
        
        check!(response.status().is_success());
        
        // Small delay between requests
        if i < 9 {
            sleep(Duration::from_millis(100)).await;
        }
    }
    
    Ok(())
}
```

## Maintenance

### Keep Tests Up to Date
Regularly review and update tests as APIs evolve:

- Update endpoint URLs when they change
- Modify assertions when response formats change  
- Add tests for new API features
- Remove tests for deprecated functionality

### Document Complex Test Logic
Add comments for complex test scenarios:

```rust
#[tanu::test]
async fn complex_workflow_test() -> eyre::Result<()> {
    // Step 1: Create user account
    let user_response = client.post("/users").json(&user_data).send().await?;
    let user_id = user_response.json::<serde_json::Value>().await?["id"].as_i64().unwrap();
    
    // Step 2: Verify email (simulated)
    client.post(&format!("/users/{}/verify", user_id)).send().await?;
    
    // Step 3: Login with verified account
    let login_response = client
        .post("/auth/login")
        .json(&login_data)
        .send()
        .await?;
    
    check!(login_response.status().is_success(), "Login should succeed after verification");
    
    Ok(())
}
```

### Regular Cleanup
- Remove obsolete tests
- Consolidate duplicate test logic
- Update dependencies regularly
- Review and update configuration files

### Result Type Flexibility

Tanu supports various Result types, allowing you to choose the error handling approach that best fits your needs:

```rust
// eyre::Result (recommended)
#[tanu::test]
async fn test_with_eyre() -> eyre::Result<()> {
    let response = client.get("/api/endpoint").send().await?;
    check!(response.status().is_success());
    Ok(())
}

// anyhow::Result  
#[tanu::test]
async fn test_with_anyhow() -> anyhow::Result<()> {
    let response = client.get("/api/endpoint").send().await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Expected success, got: {}", response.status()));
    }
    Ok(())
}

// Custom error types
#[derive(Debug)]
enum ApiError {
    Network(String),
    InvalidResponse(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(msg) => write!(f, "Network error: {}", msg),
            ApiError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

#[tanu::test]
async fn test_with_custom_error() -> Result<(), ApiError> {
    let response = client.get("/api/endpoint").send().await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    
    if !response.status().is_success() {
        return Err(ApiError::InvalidResponse(
            format!("Status: {}", response.status())
        ));
    }
    Ok(())
}

// Simple string errors
#[tanu::test]
async fn test_with_string_error() -> Result<(), String> {
    let response = client.get("/api/endpoint").send().await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Expected success, got: {}", response.status()));
    }
    Ok(())
}
```

**Recommendation: Use `eyre::Result` for best experience**

While tanu supports `anyhow::Result`, `std::result::Result`, and custom error types, we **strongly recommend using `eyre::Result`** for the following reasons:

- **Seamless integration**: Tanu's `check!`, `check_eq!`, and other assertion macros return `eyre::Result`, providing perfect compatibility
- **Colored backtraces**: eyre provides beautiful, colored error backtraces that make debugging much easier
- **Rich error context**: eyre excels at capturing and displaying error context chains
- **Zero friction**: No need for manual error conversions or custom assertion logic
- **Consistent experience**: Best integration with tanu's error reporting and TUI

**Alternative Result types:**
- **Use `anyhow::Result`** when you need compatibility with existing anyhow-based code (requires manual assertions)
- **Use custom error types** when you want specific error categorization or need to implement particular error handling logic
- **Use `Result<(), String>`** for simple tests where detailed error handling isn't critical

**Important:** When using non-eyre Result types, you cannot use tanu's `check!` macros directly since they return `eyre::Result`. You'll need to write manual assertions as shown in the examples above.

By following these best practices, you'll create maintainable, reliable, and effective API tests with tanu.
