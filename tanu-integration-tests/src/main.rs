mod assertion;
#[cfg(feature = "fail-test")]
mod fail_test;
mod grpc;
mod http;
mod macros;
mod misc;
mod retry;
mod task_local;
mod tcp;

use std::sync::Arc;

use tanu::eyre;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    ContainerAsync, GenericImage, ImageExt,
};
use tokio::sync::OnceCell;

/// A static instance of the httpbin container.
static HTTPBIN: OnceCell<Arc<HttpBin>> = OnceCell::const_new();

#[derive(Clone)]
pub struct HttpBin {
    container: Arc<ContainerAsync<GenericImage>>,
}

impl HttpBin {
    pub async fn get_base_url(&self) -> String {
        let host = self.container.get_host().await.unwrap();
        let port = self.container.get_host_port_ipv4(80.tcp()).await.unwrap();
        format!("http://{host}:{port}")
    }
}

/// To stop container on shutdown, we need to use a destructor.
/// See https://github.com/testcontainers/testcontainers-rs/issues/707#issuecomment-2290834813
#[dtor::dtor]
fn on_shutdown() {
    if let Some(container_id) = HTTPBIN.get().map(|c| c.container.id()) {
        std::process::Command::new("docker")
            .args(["container", "rm", "-f", container_id])
            .output()
            .expect("failed to stop testcontainer");
    }
}

pub async fn get_httpbin() -> eyre::Result<Arc<HttpBin>> {
    let httpbin = HTTPBIN
        .get_or_init(|| async {
            let container = GenericImage::new("kennethreitz/httpbin", "latest")
                .with_exposed_port(80.tcp())
                .with_wait_for(WaitFor::message_on_stderr("Using worker: gevent"))
                .with_network("bridge")
                .start()
                .await
                .expect("failed to start httpbin");
            Arc::new(HttpBin {
                container: container.into(),
            })
        })
        .await;
    Ok(httpbin.clone())
}

/// Get the base URL for HTTP tests.
///
/// If `base_url` is configured in the project, use it directly.
/// Otherwise, start a Docker httpbin container and use its URL.
pub async fn get_base_url() -> eyre::Result<String> {
    let config = tanu::get_config();
    if let Ok(base_url) = config.get_str("base_url") {
        if !base_url.is_empty() {
            return Ok(base_url.to_string());
        }
    }
    // Fall back to Docker container
    Ok(get_httpbin().await?.get_base_url().await)
}

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await?;
    Ok(())
}
