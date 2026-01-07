use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PutResponse {
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
struct PutPayload {
    id: i32,
    name: String,
    value: String,
}

#[tanu::test]
async fn put_json() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let payload = PutPayload {
        id: 1,
        name: "test".to_string(),
        value: "updated".to_string(),
    };

    let res = http
        .put(format!("{base_url}/put"))
        .json(&payload)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PutResponse = res.json().await?;

    if let Some(json) = response.json {
        check_eq!(1, json["id"].as_i64().unwrap());
        check_eq!("test", json["name"].as_str().unwrap());
        check_eq!("updated", json["value"].as_str().unwrap());
    } else {
        check!(false, "Expected JSON payload in response");
    }

    Ok(())
}

#[tanu::test]
async fn put_form_data() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let params = [("name", "resource"), ("status", "active")];
    let res = http
        .put(format!("{base_url}/put"))
        .form(&params)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PutResponse = res.json().await?;

    check_eq!("resource", response.form.get("name").unwrap());
    check_eq!("active", response.form.get("status").unwrap());

    Ok(())
}

#[tanu::test]
async fn put_text_data() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let text_data = "This is plain text data for PUT request";

    let res = http
        .put(format!("{base_url}/put"))
        .header("content-type", "text/plain")
        .body(text_data)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PutResponse = res.json().await?;

    check_eq!(text_data, response.data);

    Ok(())
}

#[tanu::test]
async fn put_with_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let payload = PutPayload {
        id: 42,
        name: "test_resource".to_string(),
        value: "query_test".to_string(),
    };

    let res = http
        .put(format!("{base_url}/put"))
        .query(&[("version", "1.0"), ("force", "true")])
        .json(&payload)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: PutResponse = res.json().await?;

    check_eq!("1.0", response.args.get("version").unwrap());
    check_eq!("true", response.args.get("force").unwrap());

    if let Some(json) = response.json {
        check_eq!(42, json["id"].as_i64().unwrap());
        check_eq!("test_resource", json["name"].as_str().unwrap());
        check_eq!("query_test", json["value"].as_str().unwrap());
    }

    Ok(())
}
