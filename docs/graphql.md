# GraphQL Testing

---
tags:
  - GraphQL
  - HTTP
  - Testing
  - API
---

Tanu provides an ergonomic GraphQL client built on top of the existing HTTP layer. It supports two modes: flexible runtime string queries for quick and error-path testing, and type-safe codegen queries via [`graphql_client`](https://github.com/graphql-rust/graphql-client) for schema-validated tests.

## Installation

Enable the `graphql` feature flag in your `Cargo.toml`:

```toml
[dependencies]
tanu = { version = "0.20.0", features = ["graphql"] }
```

## Quick Start

Send a GraphQL query at runtime without a schema:

```rust
use tanu::{check, check_eq, eyre, graphql, http::Client};

#[tanu::test]
async fn get_users() -> eyre::Result<()> {
    let client = Client::new();
    let res = client
        .graphql("https://api.example.com/graphql")
        .query("{ users { id name } }")
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());

    let data: graphql::Response<serde_json::Value> = res.json().await?;
    check!(data.errors.is_none());
    Ok(())
}
```

## Variables & Operation Name

Pass variables and an operation name alongside your query:

```rust
let res = client
    .graphql("https://api.example.com/graphql")
    .query("query GetUser($id: ID!) { user(id: $id) { name email } }")
    .variables(serde_json::json!({"id": "42"}))
    .operation_name("GetUser")
    .send()
    .await?;
```

## Type-Safe Queries

For stricter validation, use `#[derive(GraphQLQuery)]` from the `graphql_client` crate to generate Rust types from your `.graphql` files and a schema.

Add `graphql_client` to your dependencies:

```toml
[dependencies]
graphql_client = "0.16"
```

Define your query and schema files, then derive the query:

```rust
use graphql_client::GraphQLQuery;
use tanu::{check, check_eq, eyre, graphql, http::Client};

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.graphql",
    query_path = "src/graphql/get_user.graphql",
    response_derives = "Debug",
)]
struct GetUser;

#[tanu::test]
async fn test_get_user() -> eyre::Result<()> {
    let client = Client::new();
    let res = client
        .graphql("https://api.example.com/graphql")
        .typed_query::<GetUser>(get_user::Variables { id: "42".to_string() })
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());

    let data: graphql::Response<get_user::ResponseData> = res.json().await?;
    check!(data.errors.is_none());
    check!(data.data.is_some());
    Ok(())
}
```

## Response Handling

`graphql::Response<T>` is a type-safe wrapper that mirrors the [GraphQL over HTTP spec](https://graphql.github.io/graphql-over-http/):

```rust
let gql_res: graphql::Response<serde_json::Value> = res.json().await?;

// Check for GraphQL-level errors
if let Some(errors) = &gql_res.errors {
    for err in errors {
        eprintln!("GraphQL error: {}", err.message);
    }
}

// Access response data
if let Some(data) = gql_res.data {
    let users = &data["users"];
    check!(users.is_array());
}
```

## Authentication

Use Bearer tokens or Basic auth — both delegate to the underlying HTTP builder:

```rust
// Bearer token
let res = client
    .graphql("https://api.example.com/graphql")
    .query("{ me { id name } }")
    .bearer_auth("your-access-token")
    .send()
    .await?;

// Basic auth
let res = client
    .graphql("https://api.example.com/graphql")
    .query("{ me { id } }")
    .basic_auth("username", Some("password"))
    .send()
    .await?;

// Custom header
let res = client
    .graphql("https://api.example.com/graphql")
    .query("{ me { id } }")
    .header("X-Api-Key", "secret")
    .send()
    .await?;
```

## Error Testing

Test how your API handles invalid queries or unauthorized requests:

```rust
#[tanu::test]
async fn test_malformed_query() -> eyre::Result<()> {
    let client = Client::new();
    let res = client
        .graphql("https://api.example.com/graphql")
        .query("{ invalid syntax !!!")
        .send()
        .await?;

    // GraphQL servers typically return 200 with errors in the body
    check_eq!(200, res.status().as_u16());

    let gql_res: graphql::Response<serde_json::Value> = res.json().await?;
    check!(gql_res.errors.is_some(), "Expected GraphQL errors for invalid query");
    Ok(())
}

#[tanu::test]
async fn test_unauthorized() -> eyre::Result<()> {
    let client = Client::new();
    let res = client
        .graphql("https://api.example.com/graphql")
        .query("{ adminOnlyData { secret } }")
        .send()
        .await?;

    let gql_res: graphql::Response<serde_json::Value> = res.json().await?;
    check!(gql_res.errors.is_some(), "Expected authorization error");
    Ok(())
}
```

## Best Practices

**Use runtime string queries** (`.query()`) when:

- Writing quick exploratory tests
- Testing error cases with intentionally invalid queries
- The query structure changes between test runs

**Use type-safe codegen** (`.typed_query::<Q>()`) when:

- You have a stable schema and want compile-time validation
- Testing production query logic that mirrors your application code
- Refactoring queries and want the compiler to catch breakage

**Validate both `data` and `errors`**: GraphQL servers can return partial data alongside errors. Always check both fields rather than assuming a 200 status means success.

```rust
let gql_res: graphql::Response<serde_json::Value> = res.json().await?;
check!(gql_res.errors.is_none(), "Unexpected GraphQL errors");
check!(gql_res.data.is_some(), "Expected data in response");
```
