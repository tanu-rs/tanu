use tanu::{check, eyre, http::ApiClient};

#[tanu::test]
async fn get() -> eyre::Result<()> {
    let http = ApiClient::new();
    let res = http.get("/get").send().await?;
    check!(res.status().is_success());
    Ok(())
}
