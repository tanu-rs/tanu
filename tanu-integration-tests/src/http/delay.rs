use serde::Deserialize;
use std::collections::HashMap;
use std::time::Instant;
use tanu::{check, check_eq, eyre, http::Client};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct DelayResponse {
    args: HashMap<String, String>,
    headers: HashMap<String, String>,
    origin: String,
    url: String,
}

#[tanu::test(1)]
#[tanu::test(2)]
async fn delay_seconds(seconds: u64) -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let start = Instant::now();
    let res = http
        .get(format!("{base_url}/delay/{seconds}"))
        .send()
        .await?;
    let duration = start.elapsed();

    check!(res.status().is_success(), "Non 2xx status received");
    check!(
        duration.as_secs() >= seconds,
        "Request should take at least {seconds} second(s)"
    );

    let response: DelayResponse = res.json().await?;
    check_eq!(format!("{base_url}/delay/{seconds}"), response.url);

    Ok(())
}

#[tanu::test]
async fn delay_with_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let start = Instant::now();
    let res = http
        .get(format!("{base_url}/delay/1"))
        .query(&[("param1", "value1"), ("param2", "value2")])
        .send()
        .await?;
    let duration = start.elapsed();

    check!(res.status().is_success(), "Non 2xx status received");
    check!(
        duration.as_secs() >= 1,
        "Request should take at least 1 second"
    );

    let response: DelayResponse = res.json().await?;
    check_eq!("value1", response.args.get("param1").unwrap());
    check_eq!("value2", response.args.get("param2").unwrap());

    Ok(())
}

#[tanu::test]
async fn delay_with_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let start = Instant::now();
    let res = http
        .get(format!("{base_url}/delay/1"))
        .header("x-test-header", "delay-test")
        .send()
        .await?;
    let duration = start.elapsed();

    check!(res.status().is_success(), "Non 2xx status received");
    check!(
        duration.as_secs() >= 1,
        "Request should take at least 1 second"
    );

    let response: DelayResponse = res.json().await?;
    check_eq!("delay-test", response.headers.get("X-Test-Header").unwrap());

    Ok(())
}

#[tanu::test]
async fn delay_post_request() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let start = Instant::now();
    let res = http
        .post(format!("{base_url}/delay/1"))
        .json(&serde_json::json!({"test": "data"}))
        .send()
        .await?;
    let duration = start.elapsed();

    check!(res.status().is_success(), "Non 2xx status received");
    check!(
        duration.as_secs() >= 1,
        "Request should take at least 1 second"
    );

    Ok(())
}
