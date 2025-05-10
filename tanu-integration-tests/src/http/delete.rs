// filepath: /home/yukinari/repos/r/tanu/tanu-integration-tests/src/http/delete.rs
use serde::Deserialize;
use std::collections::HashMap;
use tanu::{
    assert, assert_eq, eyre,
    http::{Client, StatusCode},
};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct DeleteResponse {
    args: HashMap<String, String>,
    data: String,
    files: HashMap<String, String>,
    form: HashMap<String, String>,
    headers: HashMap<String, String>,
    json: Option<serde_json::Value>,
    origin: String,
    url: String,
}

#[tanu::test]
async fn delete_resource() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.delete(format!("{base_url}/delete")).send().await?;

    assert!(res.status().is_success(), "Non 2xx status received");
    assert_eq!(StatusCode::OK, res.status());

    let response: DeleteResponse = res.json().await?;
    assert_eq!(format!("{base_url}/delete"), response.url);

    Ok(())
}

#[tanu::test]
async fn delete_with_body() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let body = r#"{"resource_id": 123}"#;

    let res = http
        .delete(format!("{base_url}/delete"))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;

    assert!(res.status().is_success(), "Non 2xx status received");

    let response: DeleteResponse = res.json().await?;

    // Verify the request body was sent correctly
    assert_eq!(body, response.data);

    Ok(())
}

#[tanu::test]
async fn delete_with_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http
        .delete(format!("{base_url}/delete"))
        .query(&[("confirm", "true"), ("cascade", "true")])
        .send()
        .await?;

    assert!(res.status().is_success(), "Non 2xx status received");

    let response: DeleteResponse = res.json().await?;

    // Verify query parameters were sent correctly
    assert_eq!("true", response.args.get("confirm").unwrap());
    assert_eq!("true", response.args.get("cascade").unwrap());

    Ok(())
}
