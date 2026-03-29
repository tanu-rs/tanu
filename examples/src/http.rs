use tanu::{check, eyre, http::Client, http::HttpClient};

#[tanu::test]
async fn get() -> eyre::Result<()> {
    let http = Client::new();
    let res = http.get("https://httpbin.org/get").send().await?;
    check!(res.status().is_success());
    Ok(())
}
