use serde::Deserialize;
use std::collections::HashMap;
use tanu::{
    check, check_eq, eyre,
    http::{Client, StatusCode},
};

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
    let base_url = crate::get_base_url().await?;

    let res = http.get(format!("{base_url}/get")).send().await?;
    check!(res.status().is_success(), "Non 2xx satus received");

    let payload: Payload = res.json().await?;
    check!(payload.args.is_empty());
    check!(!payload.headers.is_empty());
    check!(!payload.origin.is_empty());
    check_eq!(format!("{base_url}/get"), payload.url.as_str());
    Ok(())
}

#[tanu::test]
async fn basic_auth() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/basic-auth/user/password"))
        .basic_auth("user", Some("password"))
        .send()
        .await?;
    check!(res.status().is_success(), "Non 2xx satus received");

    let payload: BasicAuthPayload = res.json().await?;
    check!(payload.authenticated);
    check_eq!("user", payload.user);
    Ok(())
}

#[tanu::test]
async fn basic_auth_error() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/basic-auth/user/password"))
        .basic_auth("user", Some("wrong-password"))
        .send()
        .await?;
    check_eq!(StatusCode::UNAUTHORIZED, res.status());
    Ok(())
}

#[tanu::test]
async fn bearer_auth() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/bearer"))
        .bearer_auth("token")
        .send()
        .await?;
    check!(res.status().is_success(), "Non 2xx satus received");

    let payload: BearerAuthPayload = res.json().await?;
    check!(payload.authenticated);
    check_eq!("token", payload.token);
    Ok(())
}

#[tanu::test]
async fn same_test_name_in_different_modules() -> eyre::Result<()> {
    Ok(())
}
