// filepath: /home/yukinari/repos/r/tanu/tanu-integration-tests/src/http/patch.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PatchResponse {
    args: HashMap<String, String>,
    data: String,
    files: HashMap<String, String>,
    form: HashMap<String, String>,
    headers: HashMap<String, String>,
    json: Option<serde_json::Value>,
    origin: String,
    url: String,
}

#[derive(Debug, Serialize)]
struct PatchPayload {
    id: i32,
    updated_field: String,
}

#[tanu::test]
async fn patch_json() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let payload = PatchPayload {
        id: 1,
        updated_field: "patched value".to_string(),
    };

    let res = http
        .patch(format!("{base_url}/patch"))
        .json(&payload)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PatchResponse = res.json().await?;

    // Verify the JSON payload was sent correctly
    if let Some(json) = response.json {
        check_eq!(1, json["id"].as_i64().unwrap());
        check_eq!("patched value", json["updated_field"].as_str().unwrap());
    } else {
        check!(false, "Expected JSON payload in response");
    }

    Ok(())
}

#[tanu::test]
async fn patch_with_headers() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let payload = r#"{"partial": true, "data": "partial update"}"#;

    let res = http
        .patch(format!("{base_url}/patch"))
        .header("x-custom-header", "patch-test")
        .header("content-type", "application/json")
        .body(payload)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PatchResponse = res.json().await?;

    // Verify headers were sent correctly
    check!(response.headers.contains_key("X-Custom-Header"));
    check_eq!(
        "patch-test",
        response.headers.get("X-Custom-Header").unwrap()
    );

    Ok(())
}
