// NOTE: This is a demonstration of gRPC testing with Tanu.
// These examples are ignored by default since they require a gRPC server.
// See tanu-integration-tests/src/grpc.rs for complete working examples.

use tanu::{eyre, grpc};

// Example: Testing a gRPC echo service
// This shows the basic pattern for testing gRPC endpoints with Tanu
#[tanu::test]
#[ignore = "requires gRPC server at localhost:50051"]
async fn grpc_example() -> eyre::Result<()> {
    // Step 1: Connect to the gRPC server using grpc::connect()
    // This automatically enables request/response logging via middleware
    let channel = grpc::connect("http://localhost:50051").await?;

    // Step 2: Create your gRPC client from the channel
    // let mut client = YourServiceClient::new(channel);
    let _client = channel; // Placeholder to use the channel variable

    // Step 3: Make gRPC calls - they will be automatically logged
    // let response = client.your_method(YourRequest { ... }).await?.into_inner();

    // Step 4: Use Tanu's assertion macros to verify responses
    // check_eq!(expected_value, response.field);
    // check!(response.status == Status::Ok);

    Ok(())
}

// Example: Testing with metadata (headers in gRPC)
#[tanu::test]
#[ignore = "requires gRPC server at localhost:50051"]
async fn grpc_with_metadata() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let _client = channel; // Placeholder

    // You can add metadata to requests:
    // let mut request = tonic::Request::new(YourRequest { ... });
    // request.metadata_mut().insert("authorization", "Bearer token".parse()?);
    // let response = client.your_method(request).await?;

    Ok(())
}

// Example: Testing error cases
#[tanu::test]
#[ignore = "requires gRPC server at localhost:50051"]
async fn grpc_error_handling() -> eyre::Result<()> {
    let channel = grpc::connect("http://localhost:50051").await?;
    let _client = channel; // Placeholder

    // Test that errors are handled correctly:
    // let result = client.invalid_method(InvalidRequest { ... }).await;
    // check!(result.is_err());
    // let status = result.unwrap_err();
    // check_eq!(tonic::Code::InvalidArgument, status.code());

    Ok(())
}
