use base64::{engine::general_purpose, Engine as _};
use tanu::{check, check_eq, eyre, http::Client};

#[tanu::test("Hello, World!")]
#[tanu::test("Test data")]
#[tanu::test("Simple test")]
async fn base64_decode(text: &str) -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let encoded = general_purpose::STANDARD.encode(text);
    let res = http
        .get(format!("{base_url}/base64/{encoded}"))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let body = res.text().await?;
    check_eq!(text, body);

    Ok(())
}

#[tanu::test]
async fn utf8_content() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/encoding/utf8")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let body = res.text().await?;
    check!(body.contains("UTF-8"));

    Ok(())
}

#[tanu::test]
async fn json_utf8() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http
        .post(format!("{base_url}/post"))
        .json(&serde_json::json!({
            "text": "Hello, 世界! 🌍",
            "emoji": "👋🌟",
            "unicode": "café naïve résumé"
        }))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: serde_json::Value = res.json().await?;
    let json_data = response["json"].as_object().unwrap();

    check_eq!("Hello, 世界! 🌍", json_data["text"].as_str().unwrap());
    check_eq!("👋🌟", json_data["emoji"].as_str().unwrap());
    check_eq!("café naïve résumé", json_data["unicode"].as_str().unwrap());

    Ok(())
}

#[tanu::test]
async fn xml_content() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let xml_data = r#"<?xml version="1.0" encoding="UTF-8"?>
<test>
    <message>Hello, XML!</message>
    <number>42</number>
</test>"#;

    let res = http
        .post(format!("{base_url}/post"))
        .header("content-type", "application/xml")
        .body(xml_data)
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: serde_json::Value = res.json().await?;
    let data = response["data"].as_str().unwrap();

    check!(data.contains("Hello, XML!"));
    check!(data.contains("42"));

    Ok(())
}
