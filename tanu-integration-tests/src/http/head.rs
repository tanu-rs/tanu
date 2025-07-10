// filepath: /home/yukinari/repos/r/tanu/tanu-integration-tests/src/http/head.rs
use tanu::{
    check, check_eq, eyre,
    http::{Client, StatusCode},
};

#[tanu::test]
async fn head_request() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.head(format!("{base_url}/get")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");
    check_eq!(StatusCode::OK, res.status());

    // But should have headers
    check!(!res.headers().is_empty());

    // HEAD requests should have no body
    let body = res.text().await?;
    check_eq!("", body, "HEAD request should not return body content");

    Ok(())
}

#[tanu::test]
async fn head_with_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http
        .head(format!("{base_url}/get"))
        .query(&[("param1", "value1"), ("param2", "value2")])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    // Verify the URL with query parameters
    let url = res.url().to_string();
    check!(url.contains("param1=value1"));
    check!(url.contains("param2=value2"));

    Ok(())
}

#[tanu::test]
async fn head_status_codes() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    // Test with 404 Not Found
    let res = http.head(format!("{base_url}/status/404")).send().await?;

    check_eq!(StatusCode::NOT_FOUND, res.status());

    // Test with 500 Server Error
    let res = http.head(format!("{base_url}/status/500")).send().await?;

    check_eq!(StatusCode::INTERNAL_SERVER_ERROR, res.status());

    Ok(())
}
