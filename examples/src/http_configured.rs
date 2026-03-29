use tanu::{check, eyre, http::ApiClient, http::HttpClient};

#[tanu::test]
async fn api_client_get() -> eyre::Result<()> {
    let http = ApiClient::new();
    let res = http.get("/get").send().await?;
    check!(res.status().is_success());
    Ok(())
}
