use serde::Deserialize;
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[derive(Debug, Deserialize)]
struct HeadersResponse {
    headers: HashMap<String, String>,
}

#[tanu::test]
async fn response_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/response-headers"))
        .query(&[("content-type", "application/json"), ("x-custom", "test")])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let content_type = res.headers().get("content-type");
    check!(content_type.is_some());
    check!(content_type
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));

    let custom_header = res.headers().get("x-custom");
    check!(custom_header.is_some());
    check_eq!("test", custom_header.unwrap().to_str().unwrap());

    Ok(())
}

#[tanu::test]
async fn request_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/headers"))
        .header("user-agent", "tanu-test-client/1.0")
        .header("x-test-header", "test-value")
        .header("accept", "application/json")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: HeadersResponse = res.json().await?;

    check!(response.headers.contains_key("User-Agent"));
    check_eq!(
        "tanu-test-client/1.0",
        response.headers.get("User-Agent").unwrap()
    );

    check!(response.headers.contains_key("X-Test-Header"));
    check_eq!("test-value", response.headers.get("X-Test-Header").unwrap());

    check!(response.headers.contains_key("Accept"));
    check_eq!("application/json", response.headers.get("Accept").unwrap());

    Ok(())
}

#[tanu::test]
async fn cache_control_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/cache"))
        .header("cache-control", "no-cache")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response = res.json::<HeadersResponse>().await?;
    check!(response.headers.contains_key("Cache-Control"));

    // NOTE: httpbin does not return "cache-control" header by default,
    // let cache_control = res.headers().get("cache-control");
    // check!(cache_control.is_some());

    Ok(())
}

#[tanu::test]
async fn etag_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/etag/test-etag"))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let etag = res.headers().get("etag");
    check!(etag.is_some());
    check!(etag.unwrap().to_str().unwrap().contains("test-etag"));

    Ok(())
}

#[tanu::test]
async fn multiple_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/headers"))
        .header("x-header-1", "value1")
        .header("x-header-2", "value2")
        .header("x-header-3", "value3")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: HeadersResponse = res.json().await?;

    check_eq!("value1", response.headers.get("X-Header-1").unwrap());
    check_eq!("value2", response.headers.get("X-Header-2").unwrap());
    check_eq!("value3", response.headers.get("X-Header-3").unwrap());

    Ok(())
}

#[tanu::test]
async fn user_agent_header() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let custom_ua = "Custom-User-Agent/2.0 (Testing)";

    let res = http
        .get(format!("{base_url}/user-agent"))
        .header("user-agent", custom_ua)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let body = res.text().await?;
    check!(body.contains(custom_ua));

    Ok(())
}
