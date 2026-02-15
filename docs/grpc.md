# gRPC Testing

---
tags:
  - gRPC
  - Testing
  - API
---

Tanu provides automatic request/response capture for gRPC calls through Tower middleware integration. This allows seamless testing of gRPC services with full observability of method calls, metadata, status codes, and performance metrics.

## Installation

To use gRPC testing features, enable the `grpc` feature flag in your `Cargo.toml`:

```toml
[dependencies]
tanu = { version = "0.17.0", features = ["grpc"] }
tonic = "0.12"  # or your preferred version
tokio = { version = "1", features = ["full"] }
```

## Quick Start

The simplest way to get started is using `grpc::connect()`, which creates a channel with automatic logging enabled:

```rust
use tanu::{check_eq, eyre, grpc};

#[tanu::test]
async fn grpc_unary_call() -> eyre::Result<()> {
    // Connect with automatic logging
    let channel = grpc::connect("http://localhost:50051").await?;
    let mut client = MyServiceClient::new(channel);

    // All gRPC calls are automatically logged
    let response = client.unary_call(request).await?;

    check_eq!("expected", response.into_inner().message);
    Ok(())
}
```

## Using the Extension Trait

For more control over channel creation, use the `ChannelExt` trait to add logging to an existing channel:

```rust
use tanu::grpc::ChannelExt;
use tonic::transport::Channel;

#[tanu::test]
async fn with_extension_trait() -> eyre::Result<()> {
    let channel = Channel::from_static("http://localhost:50051")
        .connect()
        .await?
        .with_tanu_logging();

    let mut client = MyServiceClient::new(channel);
    let response = client.unary_call(request).await?;

    Ok(())
}
```

## Captured Data

The gRPC middleware automatically captures the following information for every call:

| Field | Description | Example |
|-------|-------------|---------|
| **Method** | Full gRPC method path | `/echo.Echo/Unary` |
| **Request Metadata** | Headers sent to the server | `x-request-id: 123` |
| **Response Metadata** | Headers received from the server | `x-response-id: 456` |
| **Status Code** | gRPC status code | `OK` (0), `INVALID_ARGUMENT` (3) |
| **Status Message** | Error message (if any) | `"missing required field"` |
| **Duration** | Request-to-response time | `125ms` |
| **Timestamps** | Start and end times | `2026-01-21T10:30:00Z` |

!!! info "Message Body Limitations"
    Due to gRPC's streaming nature, request and response message bodies are not captured by the middleware. The middleware focuses on metadata, status, and timing information.

## Testing with Metadata

You can test gRPC calls that require custom metadata:

```rust
use tonic::{Request, metadata::MetadataValue};

#[tanu::test]
async fn test_with_custom_metadata() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let mut client = EchoClient::new(channel);

    let mut request = Request::new(EchoRequest {
        message: "hello".to_string(),
    });

    // Add custom metadata
    request.metadata_mut()
        .insert("x-api-key", "secret-key".parse().unwrap());
    request.metadata_mut()
        .insert("x-request-id", "req-123".parse().unwrap());

    // Metadata is automatically captured by the middleware
    let response = client.unary(request).await?;

    check_eq!("hello", response.into_inner().message);
    Ok(())
}
```

## Error Handling

The middleware automatically captures error responses with status codes and messages:

```rust
#[tanu::test]
async fn test_invalid_request() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let mut client = EchoClient::new(channel);

    // This call will fail
    let result = client.unary(EchoRequest {
        message: "".to_string(),  // Empty message might be invalid
    }).await;

    // Error is logged automatically with status code and message
    check!(result.is_err());

    if let Err(status) = result {
        check_eq!(tonic::Code::InvalidArgument, status.code());
    }

    Ok(())
}
```

## Server Streaming

The middleware works seamlessly with streaming RPCs:

```rust
use tokio_stream::StreamExt;

#[tanu::test]
async fn test_server_streaming() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let mut client = EchoClient::new(channel);

    // Initial request is logged
    let mut stream = client.server_stream(EchoStreamRequest {
        count: 5,
        message: "hello".to_string(),
    }).await?.into_inner();

    let mut received = 0;
    while let Some(response) = stream.next().await {
        let message = response?.message;
        check_eq!("hello", message);
        received += 1;
    }

    check_eq!(5, received);
    Ok(())
}
```

## Performance Testing

Use the captured duration data for performance assertions:

```rust
#[tanu::test]
async fn test_response_time() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let mut client = EchoClient::new(channel);

    let start = std::time::Instant::now();
    let response = client.unary(request).await?;
    let duration = start.elapsed();

    // Assert response time is acceptable
    check!(duration.as_millis() < 100,
           "Response took {}ms, expected < 100ms",
           duration.as_millis());

    Ok(())
}
```

## Best Practices

### Reuse Channels

Create channels once and reuse them across multiple test calls:

```rust
use tokio::sync::OnceCell;

static CHANNEL: OnceCell<grpc::LoggingChannel> = OnceCell::const_new();

async fn get_channel() -> grpc::LoggingChannel {
    CHANNEL.get_or_init(|| async {
        grpc::connect("http://localhost:50051")
            .await
            .expect("Failed to connect")
    }).await.clone()
}

#[tanu::test]
async fn test_one() -> eyre::Result<()> {
    let channel = get_channel().await;
    let mut client = EchoClient::new(channel);
    // ...
    Ok(())
}

#[tanu::test]
async fn test_two() -> eyre::Result<()> {
    let channel = get_channel().await;
    let mut client = EchoClient::new(channel);
    // ...
    Ok(())
}
```

### Test Server Errors

Always include tests for error scenarios:

```rust
#[tanu::test]
async fn test_missing_required_metadata() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let mut client = EchoClient::new(channel);

    // Don't add required metadata
    let err = client.unary(request).await.unwrap_err();

    check_eq!(tonic::Code::InvalidArgument, err.code());
    check!(err.message().contains("missing metadata"));

    Ok(())
}
```

### Use Descriptive Method Paths

The captured method path helps identify which call failed:

```rust
// The middleware captures: "/echo.Echo/Unary"
let response = client.unary(request).await?;

// The middleware captures: "/echo.Echo/ServerStream"
let stream = client.server_stream(request).await?;
```

## Integration with TUI

All captured gRPC calls are visible in the TUI test runner, showing:

- Method paths
- Metadata (request and response)
- Status codes and messages
- Response times
- Timestamps

Run tests in TUI mode to see detailed gRPC call information:

```bash
cargo run tui
```

## Architecture

The gRPC logging feature is built using:

- **Tower middleware**: Wraps Tonic channels with logging layer
- **Event system**: Publishes `CallLog::Grpc` events to the test runner
- **Zero overhead**: Logging only activates when tests run with capture enabled

The middleware implementation is in `tanu-core/src/grpc.rs` and integrates with the existing event-driven test runner architecture.
