use serde::Deserialize;
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CompressionResponse {
    gzipped: Option<bool>,
    deflated: Option<bool>,
    brotli: Option<bool>,
    headers: HashMap<String, String>,
    method: String,
    origin: String,
}

/// A gzip-compressed response is decoded and parsed as JSON.
#[tanu::test]
async fn gzip_compression() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/gzip"))
        .header("accept-encoding", "gzip")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CompressionResponse = res.json().await?;
    check!(response.gzipped.unwrap());
    check_eq!("GET", response.method);

    Ok(())
}

/// A deflate-compressed response is decoded and parsed as JSON.
#[tanu::test]
async fn deflate_compression() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/deflate"))
        .header("accept-encoding", "deflate")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CompressionResponse = res.json().await?;
    check!(response.deflated.unwrap());
    check_eq!("GET", response.method);

    Ok(())
}

/// A brotli-compressed response is decoded and parsed as JSON.
#[tanu::test]
async fn brotli_compression() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/brotli"))
        .header("accept-encoding", "br")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CompressionResponse = res.json().await?;
    check!(response.brotli.unwrap());
    check_eq!("GET", response.method);

    Ok(())
}

/// A gzip response is decoded when the client accepts gzip, deflate and brotli.
#[tanu::test]
async fn multiple_compression_formats() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/gzip"))
        .header("accept-encoding", "gzip, deflate, br")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CompressionResponse = res.json().await?;
    check!(response.gzipped.unwrap());

    Ok(())
}
