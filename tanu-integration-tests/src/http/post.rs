// filepath: /home/yukinari/repos/r/tanu/tanu-integration-tests/src/http/post.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tanu::{assert, assert_eq, eyre, http::Client};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PostResponse {
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
struct JsonPayload {
    name: String,
    value: i32,
}

#[tanu::test]
async fn post_json() -> eyre::Result<()> {
    let http = Client::new();
    let payload = JsonPayload {
        name: "test".to_string(),
        value: 42,
    };

    let res = http
        .post("https://httpbin.org/post")
        .json(&payload)
        .send()
        .await?;

    assert!(res.status().is_success(), "Non 2xx status received");

    let response: PostResponse = res.json().await?;

    // Verify the JSON payload was sent correctly
    if let Some(json) = response.json {
        assert_eq!("test", json["name"].as_str().unwrap());
        assert_eq!(42, json["value"].as_i64().unwrap());
    } else {
        assert!(false, "Expected JSON payload in response");
    }

    Ok(())
}

#[tanu::test]
async fn post_form() -> eyre::Result<()> {
    let http = Client::new();

    let params = [("key1", "value1"), ("key2", "value2")];
    let res = http
        .post("https://httpbin.org/post")
        .form(&params)
        .send()
        .await?;

    assert!(res.status().is_success(), "Non 2xx status received");

    let response: PostResponse = res.json().await?;

    // Verify form data was sent correctly
    assert_eq!("value1", response.form.get("key1").unwrap());
    assert_eq!("value2", response.form.get("key2").unwrap());

    Ok(())
}

#[tanu::test]
async fn post_text() -> eyre::Result<()> {
    let http = Client::new();
    let text = "Plain text payload";

    let res = http
        .post("https://httpbin.org/post")
        .body(text)
        .send()
        .await?;

    assert!(res.status().is_success(), "Non 2xx status received");

    let response: PostResponse = res.json().await?;

    // Verify text payload was sent correctly
    assert_eq!(text, response.data);

    Ok(())
}
