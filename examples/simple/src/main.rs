use serde::Deserialize;
use std::collections::HashMap;
use tanu::{assert_eq, http::Client};

#[derive(Debug, Deserialize)]
struct Payload {
    args: HashMap<String, String>,
    headers: HashMap<String, String>,
    origin: String,
    url: url::Url,
}

#[tanu::test]
async fn get() -> eyre::Result<()> {
    let http = Client::new();
    let res = http.get("https://httpbin.org/get").send().await?;
    assert!(res.status().is_success(), "Non 2xx satus received");

    let payload: Payload = res.json().await?;
    assert!(payload.args.is_empty());
    assert!(!payload.headers.is_empty());
    assert!(!payload.origin.is_empty());
    assert_eq!("https://httpbin.org/get", payload.url.as_str());
    Ok(())
}

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await?;
    Ok(())
}
