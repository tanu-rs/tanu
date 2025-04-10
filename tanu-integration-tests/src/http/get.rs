use reqwest::StatusCode;
use serde::Deserialize;
use std::collections::HashMap;
use tanu::{assert, assert_eq, eyre, http::Client};

#[derive(Debug, Deserialize)]
struct Payload {
    args: HashMap<String, String>,
    headers: HashMap<String, String>,
    origin: String,
    url: url::Url,
}

#[derive(Debug, Deserialize)]
struct BasicAuthPayload {
    authenticated: bool,
    user: String,
}

#[derive(Debug, Deserialize)]
struct BearerAuthPayload {
    authenticated: bool,
    token: String,
}

#[tanu::test]
async fn json() -> eyre::Result<()> {
    let http = Client::new();
    let res = http.get("https://httpbin.org/get").send().await?;
    assert!(res.status().is_success(), "Non 2xx satus received");

    let payload: Payload = res.json().await?;
    assert!(payload.args.is_empty());
    assert!(!payload.headers.is_empty());
    assert!(!payload.origin.is_empty());
    assert_eq!("https://httpbin.org/get", payload.url.as_str());
    Ok(())
}

#[tanu::test]
async fn basic_auth() -> eyre::Result<()> {
    let http = Client::new();
    let res = http
        .get("https://httpbin.org/basic-auth/user/password")
        .basic_auth("user", Some("password"))
        .send()
        .await?;
    assert!(res.status().is_success(), "Non 2xx satus received");

    let payload: BasicAuthPayload = res.json().await?;
    assert!(payload.authenticated);
    assert_eq!("user", payload.user);
    Ok(())
}

#[tanu::test]
async fn basic_auth_error() -> eyre::Result<()> {
    let http = Client::new();
    let res = http
        .get("https://httpbin.org/basic-auth/user/password")
        .basic_auth("user", Some("wrong-password"))
        .send()
        .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, res.status());
    Ok(())
}

#[tanu::test]
async fn bearer_auth() -> eyre::Result<()> {
    let http = Client::new();
    let res = http
        .get("https://httpbin.org/bearer")
        .bearer_auth("token")
        .send()
        .await?;
    assert!(res.status().is_success(), "Non 2xx satus received");

    let payload: BearerAuthPayload = res.json().await?;
    assert!(payload.authenticated);
    assert_eq!("token", payload.token);
    Ok(())
}
