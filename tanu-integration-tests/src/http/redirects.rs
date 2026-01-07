use serde::Deserialize;
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RedirectResponse {
    args: HashMap<String, String>,
    headers: HashMap<String, String>,
    origin: String,
    url: String,
}

#[tanu::test]
async fn redirect_to_get() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/redirect-to"))
        .query(&[("url", &format!("{base_url}/get"))])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: RedirectResponse = res.json().await?;
    check_eq!(format!("{base_url}/get"), response.url);

    Ok(())
}

#[tanu::test]
async fn relative_redirect() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/relative-redirect/3"))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: RedirectResponse = res.json().await?;
    check_eq!(format!("{base_url}/get"), response.url);

    Ok(())
}

#[tanu::test]
async fn absolute_redirect() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/absolute-redirect/2"))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: RedirectResponse = res.json().await?;
    check_eq!(format!("{base_url}/get"), response.url);

    Ok(())
}

#[tanu::test]
async fn redirect_with_status_codes() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http.get(format!("{base_url}/redirect/5")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: RedirectResponse = res.json().await?;
    check_eq!(format!("{base_url}/get"), response.url);

    Ok(())
}

#[tanu::test]
async fn redirect_with_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/redirect-to"))
        .query(&[
            ("url", &format!("{base_url}/get")),
            ("status_code", &"301".to_string()),
        ])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: RedirectResponse = res.json().await?;
    check_eq!(format!("{base_url}/get"), response.url);

    Ok(())
}
