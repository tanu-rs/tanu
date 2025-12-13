use std::{net::SocketAddr, pin::Pin};

use tanu::{check, check_eq, eyre};
use tokio::net::TcpListener;
use tokio::sync::OnceCell;
use tokio_stream::{wrappers::TcpListenerStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status};

pub mod proto {
    tonic::include_proto!("tanu.integration.echo");
}

use proto::{
    echo_client::EchoClient,
    echo_server::{Echo, EchoServer},
    EchoRequest, EchoResponse, EchoStreamRequest,
};

static GRPC_ADDR: OnceCell<SocketAddr> = OnceCell::const_new();

async fn grpc_addr() -> SocketAddr {
    *GRPC_ADDR
        .get_or_init(|| async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("failed to bind gRPC test server");
            let addr = listener
                .local_addr()
                .expect("failed to get gRPC test server address");

            tokio::spawn(async move {
                let incoming = TcpListenerStream::new(listener);
                Server::builder()
                    .add_service(EchoServer::new(EchoSvc))
                    .serve_with_incoming(incoming)
                    .await
                    .expect("gRPC test server exited unexpectedly");
            });

            addr
        })
        .await
}

#[derive(Default)]
struct EchoSvc;

#[tonic::async_trait]
impl Echo for EchoSvc {
    async fn unary(
        &self,
        request: Request<EchoRequest>,
    ) -> Result<Response<EchoResponse>, Status> {
        let required = request.get_ref().require_metadata;
        if required && !request.metadata().contains_key("x-required") {
            return Err(Status::invalid_argument("missing x-required metadata"));
        }

        let received_metadata = request
            .metadata()
            .get("x-test")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();

        Ok(Response::new(EchoResponse {
            message: request.get_ref().message.clone(),
            received_metadata,
        }))
    }

    type ServerStreamStream = Pin<
        Box<
            dyn tonic::codegen::tokio_stream::Stream<Item = Result<EchoResponse, Status>>
                + Send
                + 'static,
        >,
    >;

    async fn server_stream(
        &self,
        request: Request<EchoStreamRequest>,
    ) -> Result<Response<Self::ServerStreamStream>, Status> {
        let message = request.get_ref().message.clone();
        let count = request.get_ref().count as usize;

        let stream = tokio_stream::iter((0..count).map(move |i| {
            Ok(EchoResponse {
                message: format!("{message}-{i}"),
                received_metadata: String::new(),
            })
        }));

        Ok(Response::new(Box::pin(stream)))
    }
}

#[tanu::test]
async fn grpc_unary_echo() -> eyre::Result<()> {
    let addr = grpc_addr().await;
    let mut client = EchoClient::connect(format!("http://{addr}")).await?;

    let response = client
        .unary(EchoRequest {
            message: "hello".to_string(),
            require_metadata: false,
        })
        .await?
        .into_inner();

    check_eq!("hello", response.message);
    check!(response.received_metadata.is_empty());

    Ok(())
}

#[tanu::test]
async fn grpc_unary_metadata_roundtrip() -> eyre::Result<()> {
    let addr = grpc_addr().await;
    let mut client = EchoClient::connect(format!("http://{addr}")).await?;

    let mut request = Request::new(EchoRequest {
        message: "hello".to_string(),
        require_metadata: true,
    });
    request
        .metadata_mut()
        .insert("x-required", "ok".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-test", "metadata-value".parse().unwrap());

    let response = client.unary(request).await?.into_inner();

    check_eq!("hello", response.message);
    check_eq!("metadata-value", response.received_metadata);

    Ok(())
}

#[tanu::test]
async fn grpc_unary_missing_required_metadata_is_invalid_argument() -> eyre::Result<()> {
    let addr = grpc_addr().await;
    let mut client = EchoClient::connect(format!("http://{addr}")).await?;

    let err = client
        .unary(EchoRequest {
            message: "hello".to_string(),
            require_metadata: true,
        })
        .await
        .expect_err("expected invalid argument error");

    check_eq!(tonic::Code::InvalidArgument, err.code());

    Ok(())
}

#[tanu::test]
async fn grpc_server_streaming() -> eyre::Result<()> {
    let addr = grpc_addr().await;
    let mut client = EchoClient::connect(format!("http://{addr}")).await?;

    let mut stream = client
        .server_stream(EchoStreamRequest {
            message: "stream".to_string(),
            count: 3,
        })
        .await?
        .into_inner();

    let mut messages = Vec::new();
    while let Some(item) = stream.next().await {
        let item = item?;
        messages.push(item.message);
    }

    check_eq!(3, messages.len());
    check_eq!("stream-0", messages[0]);
    check_eq!("stream-1", messages[1]);
    check_eq!("stream-2", messages[2]);

    Ok(())
}
