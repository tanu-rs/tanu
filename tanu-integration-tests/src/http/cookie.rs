use serde::Deserialize;
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[derive(Debug, Deserialize)]
struct CookieResponse {
    cookies: HashMap<String, String>,
}

#[tanu::test]
async fn set_cookie() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/cookies/set/test_cookie/test_value"))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CookieResponse = res.json().await?;
    check_eq!("test_value", response.cookies.get("test_cookie").unwrap());

    Ok(())
}

#[tanu::test]
async fn set_multiple_cookies() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/cookies/set"))
        .query(&[("session", "abc123"), ("user", "john_doe")])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CookieResponse = res.json().await?;
    check_eq!("abc123", response.cookies.get("session").unwrap());
    check_eq!("john_doe", response.cookies.get("user").unwrap());

    Ok(())
}

#[tanu::test]
async fn delete_cookie() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/cookies/delete"))
        .query(&[("test_cookie", "")])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CookieResponse = res.json().await?;
    check!(!response.cookies.contains_key("test_cookie"));

    Ok(())
}

#[tanu::test]
async fn get_cookies() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/cookies"))
        .header("cookie", "custom_cookie=custom_value; another=value2")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: CookieResponse = res.json().await?;
    check_eq!(
        "custom_value",
        response.cookies.get("custom_cookie").unwrap()
    );
    check_eq!("value2", response.cookies.get("another").unwrap());

    Ok(())
}
