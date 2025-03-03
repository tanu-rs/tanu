use reqwest::StatusCode;
use tanu::{assert_eq, eyre, http::Client};

#[tanu::test]
async fn without_parameters() -> eyre::Result<()> {
    Ok(())
}

#[tanu::test(1)]
async fn with_integer(_: u8) -> eyre::Result<()> {
    Ok(())
}

#[tanu::test(1.0)]
async fn with_float(_: f64) -> eyre::Result<()> {
    Ok(())
}

#[tanu::test("foo")]
async fn with_string(_: &str) -> eyre::Result<()> {
    Ok(())
}

#[tanu::test(true)]
async fn with_boolean(_: bool) -> eyre::Result<()> {
    Ok(())
}

#[tanu::test(Some(StatusCode::OK))]
#[tanu::test(None)]
async fn with_optional_parameters(status: Option<StatusCode>) -> eyre::Result<()> {
    let http = Client::new();
    let res = http.get("https://httpbin.org/get").send().await?;
    if status.is_some() {
        assert_eq!(status, Some(res.status()));
    }
    Ok(())
}

#[tanu::test(1; "with_test_name_specified")]
async fn with_test_name(_n: u8) -> eyre::Result<()> {
    Ok(())
}
