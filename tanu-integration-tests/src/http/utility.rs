use serde::Deserialize;
use tanu::{check, check_eq, eyre, http::Client};

#[derive(Debug, Deserialize)]
struct IpResponse {
    origin: String,
}

#[derive(Debug, Deserialize)]
struct UuidResponse {
    uuid: String,
}

#[tanu::test]
async fn ip_address() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http.get(format!("{base_url}/ip")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: IpResponse = res.json().await?;
    check!(!response.origin.is_empty());

    Ok(())
}

#[tanu::test]
async fn uuid_generation() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http.get(format!("{base_url}/uuid")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: UuidResponse = res.json().await?;
    check!(!response.uuid.is_empty());
    check!(response.uuid.contains("-"));

    Ok(())
}

#[tanu::test]
async fn html_response() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http.get(format!("{base_url}/html")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let content_type = res.headers().get("content-type");
    check!(content_type.is_some());
    check!(content_type
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/html"));

    let body = res.text().await?;
    check!(body.contains("<html>"));
    check!(body.contains("</html>"));

    Ok(())
}

#[tanu::test]
async fn robots_txt() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http.get(format!("{base_url}/robots.txt")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let body = res.text().await?;
    check!(body.contains("User-agent"));

    Ok(())
}

#[tanu::test]
async fn anything_endpoint() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .post(format!("{base_url}/anything"))
        .json(&serde_json::json!({
            "key": "value",
            "number": 42
        }))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: serde_json::Value = res.json().await?;
    check_eq!("POST", response["method"].as_str().unwrap());
    check_eq!("value", response["json"]["key"].as_str().unwrap());
    check_eq!(42, response["json"]["number"].as_i64().unwrap());

    Ok(())
}
