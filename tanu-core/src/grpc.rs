//! # gRPC Client Module
//!
//! Tanu's gRPC logging provides automatic request/response capture for tonic clients
//! via tower middleware.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tanu::grpc;
//!
//! // Connect with automatic logging
//! let channel = grpc::connect("http://localhost:50051").await?;
//! let mut client = MyServiceClient::new(channel);
//!
//! // All calls are automatically logged
//! let response = client.unary(request).await?;
//! ```
//!
//! Or use the extension trait:
//!
//! ```rust,ignore
//! use tanu::grpc::ChannelExt;
//! use tonic::transport::Channel;
//!
//! let channel = Channel::from_static("http://localhost:50051")
//!     .connect()
//!     .await?
//!     .with_tanu_logging();
//!
//! let mut client = MyServiceClient::new(channel);
//! ```

use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime};
use tonic::body::Body;
use tonic::codegen::http::{Request, Response};
use tonic::metadata::MetadataMap;
use tonic::transport::{Channel, Endpoint};
use tower::{Layer, Service, ServiceExt};

/// Captured gRPC request data.
#[derive(Debug, Clone)]
pub struct LogRequest {
    /// Full gRPC method path (e.g., "/tanu.integration.echo.Echo/Unary")
    pub method: String,
    /// Request metadata (similar to HTTP headers)
    pub metadata: MetadataMap,
    /// Serialized request message as bytes (protobuf-encoded)
    pub message: Bytes,
}

/// Captured gRPC response data.
#[derive(Debug, Clone)]
pub struct LogResponse {
    /// Response metadata (initial metadata from server)
    pub metadata: MetadataMap,
    /// Serialized response message as bytes (protobuf-encoded)
    pub message: Bytes,
    /// gRPC status code (0 = OK, non-zero = error)
    pub status_code: tonic::Code,
    /// gRPC status message (empty for successful calls)
    pub status_message: String,
    /// Request-to-response duration
    pub duration: Duration,
}

/// Complete gRPC call log with timing information.
#[derive(Debug, Clone)]
pub struct Log {
    pub request: LogRequest,
    pub response: LogResponse,
    pub started_at: SystemTime,
    pub ended_at: SystemTime,
}

/// Error types specific to gRPC operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gRPC status error: {0}")]
    Status(#[from] tonic::Status),
    #[error("invalid URI: {0}")]
    InvalidUri(String),
}

/// Tower Layer that adds logging to gRPC services.
#[derive(Clone, Default)]
pub struct LoggingLayer;

impl LoggingLayer {
    /// Create a new logging layer.
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoggingService { inner }
    }
}

/// Tower Service that wraps gRPC calls with logging.
#[derive(Clone)]
pub struct LoggingService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for LoggingService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Error: std::fmt::Debug + Send,
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Clone the service for use in the async block
        let mut inner = self.inner.clone();
        // Swap so this instance has the ready state
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let started_at = SystemTime::now();
            let start = Instant::now();

            // Extract request info before forwarding
            let method = req.uri().path().to_string();
            let request_metadata = extract_metadata_from_headers(req.headers());

            // Ensure the cloned service is ready before calling
            // Using ready() properly awaits poll_ready
            let ready_svc = inner.ready().await.map_err(|e| {
                tracing::error!("gRPC service not ready: {:?}", e);
                e
            })?;

            // Forward the request
            let response = ready_svc.call(req).await?;

            // Capture response info
            let ended_at = SystemTime::now();
            let duration = start.elapsed();
            let response_metadata = extract_metadata_from_headers(response.headers());
            let (status_code, status_message) = extract_grpc_status(response.headers());

            // Build and publish the log
            let log = Log {
                request: LogRequest {
                    method,
                    metadata: request_metadata,
                    message: Bytes::new(), // Body is streaming, not easily captured
                },
                response: LogResponse {
                    metadata: response_metadata,
                    message: Bytes::new(), // Body is streaming, not easily captured
                    status_code,
                    status_message,
                    duration,
                },
                started_at,
                ended_at,
            };

            let _ = crate::runner::publish(crate::runner::EventBody::Call(
                crate::runner::CallLog::Grpc(Box::new(log)),
            ));

            Ok(response)
        })
    }
}

/// Type alias for a channel with logging applied.
pub type LoggingChannel = LoggingService<Channel>;

/// Extract tonic MetadataMap from HTTP headers.
fn extract_metadata_from_headers(headers: &http::HeaderMap) -> MetadataMap {
    use tonic::metadata::{AsciiMetadataKey, AsciiMetadataValue};

    let mut metadata = MetadataMap::new();
    for (key, value) in headers.iter() {
        // Skip pseudo-headers and binary metadata for now
        if !key.as_str().starts_with(':') && !key.as_str().ends_with("-bin") {
            if let Ok(value_str) = value.to_str() {
                if let Ok(name) = key.as_str().parse::<AsciiMetadataKey>() {
                    if let Ok(val) = value_str.parse::<AsciiMetadataValue>() {
                        metadata.insert(name, val);
                    }
                }
            }
        }
    }
    metadata
}

/// Extract gRPC status code and message from response headers/trailers.
fn extract_grpc_status(headers: &http::HeaderMap) -> (tonic::Code, String) {
    // gRPC status is typically in trailers, but for unary calls it may be in headers
    let code = headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .map(tonic::Code::from)
        .unwrap_or(tonic::Code::Ok);

    let message = headers
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .map(|s| urlencoding::decode(s).unwrap_or_default().into_owned())
        .unwrap_or_default();

    (code, message)
}

/// Extension trait for adding tanu logging to a gRPC channel.
pub trait ChannelExt: Sized {
    /// Wrap this channel with tanu's logging middleware.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use tanu::grpc::ChannelExt;
    /// use tonic::transport::Channel;
    ///
    /// let channel = Channel::from_static("http://localhost:50051")
    ///     .connect()
    ///     .await?
    ///     .with_tanu_logging();
    /// ```
    fn with_tanu_logging(self) -> LoggingChannel;
}

impl ChannelExt for Channel {
    fn with_tanu_logging(self) -> LoggingChannel {
        LoggingLayer::new().layer(self)
    }
}

/// Connect to a gRPC endpoint with automatic logging enabled.
///
/// This is a convenience function that creates a channel with
/// tanu's logging middleware already applied.
///
/// # Example
///
/// ```rust,ignore
/// use tanu::grpc;
///
/// let channel = grpc::connect("http://localhost:50051").await?;
/// let mut client = MyServiceClient::new(channel);
///
/// // All calls through this client are automatically logged
/// let response = client.unary(request).await?;
/// ```
pub async fn connect(endpoint: impl Into<String>) -> Result<LoggingChannel, Error> {
    let endpoint =
        Endpoint::from_shared(endpoint.into()).map_err(|e| Error::InvalidUri(e.to_string()))?;
    let channel = endpoint.connect().await?;
    Ok(channel.with_tanu_logging())
}

/// Format protobuf bytes for display.
///
/// Attempts UTF-8 decoding first, then falls back to hex dump.
pub fn format_message(bytes: &Bytes) -> String {
    if bytes.is_empty() {
        return "<empty>".to_string();
    }

    // Try UTF-8 first (for debugging with JSON-encoded protos or text)
    if let Ok(s) = std::str::from_utf8(bytes) {
        if s.chars()
            .all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
        {
            return s.to_string();
        }
    }

    // Fall back to hex dump
    let hex_lines: Vec<String> = bytes
        .chunks(16)
        .enumerate()
        .map(|(i, chunk)| {
            let hex: String = chunk.iter().map(|b| format!("{:02x} ", b)).collect();
            let ascii: String = chunk
                .iter()
                .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
                .collect();
            format!("{:04x}  {:48}  {}", i * 16, hex, ascii)
        })
        .collect();

    hex_lines.join("\n")
}
