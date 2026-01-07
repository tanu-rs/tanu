use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

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
    let base_url = crate::get_base_url().await?;

    let payload = JsonPayload {
        name: "test".to_string(),
        value: 42,
    };

    let res = http
        .post(format!("{base_url}/post"))
        .json(&payload)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PostResponse = res.json().await?;

    // Verify the JSON payload was sent correctly
    if let Some(json) = response.json {
        check_eq!("test", json["name"].as_str().unwrap());
        check_eq!(42, json["value"].as_i64().unwrap());
    } else {
        check!(false, "Expected JSON payload in response");
    }

    Ok(())
}

#[tanu::test]
async fn post_form() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let params = [("key1", "value1"), ("key2", "value2")];
    let res = http
        .post(format!("{base_url}/post"))
        .form(&params)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PostResponse = res.json().await?;

    // Verify form data was sent correctly
    check_eq!("value1", response.form.get("key1").unwrap());
    check_eq!("value2", response.form.get("key2").unwrap());

    Ok(())
}

#[tanu::test]
async fn post_text() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let text = "Plain text payload";

    let res = http
        .post(format!("{base_url}/post"))
        .body(text)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PostResponse = res.json().await?;

    // Verify text payload was sent correctly
    check_eq!(text, response.data);

    Ok(())
}
