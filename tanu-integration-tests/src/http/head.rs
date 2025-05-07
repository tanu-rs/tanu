// filepath: /home/yukinari/repos/r/tanu/tanu-integration-tests/src/http/head.rs
use tanu::{
    assert, assert_eq, eyre,
    http::{Client, StatusCode},
};

#[tanu::test]
async fn head_request() -> eyre::Result<()> {
    let http = Client::new();
    let cfg = tanu::get_config();
    let base_url = cfg.get_str("base_url")?;

    let res = http.head(format!("{base_url}/get")).send().await?;

    assert!(res.status().is_success(), "Non 2xx status received");
    assert_eq!(StatusCode::OK, res.status());

    // But should have headers
    assert!(!res.headers().is_empty());

    // HEAD requests should have no body
    let body = res.text().await?;
    assert_eq!("", body, "HEAD request should not return body content");

    Ok(())
}

#[tanu::test]
async fn head_with_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let cfg = tanu::get_config();
    let base_url = cfg.get_str("base_url")?;

    let res = http
        .head(format!("{base_url}/get"))
        .query(&[("param1", "value1"), ("param2", "value2")])
        .send()
        .await?;

    assert!(res.status().is_success(), "Non 2xx status received");

    // Verify the URL with query parameters
    let url = res.url().to_string();
    assert!(url.contains("param1=value1"));
    assert!(url.contains("param2=value2"));

    Ok(())
}

#[tanu::test]
async fn head_status_codes() -> eyre::Result<()> {
    let http = Client::new();
    let cfg = tanu::get_config();
    let base_url = cfg.get_str("base_url")?;

    // Test with 404 Not Found
    let res = http.head(format!("{base_url}/status/404")).send().await?;

    assert_eq!(StatusCode::NOT_FOUND, res.status());

    // Test with 500 Server Error
    let res = http.head(format!("{base_url}/status/500")).send().await?;

    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, res.status());

    Ok(())
}
