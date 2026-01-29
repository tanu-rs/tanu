use serde_json::Value;
use tanu::{check_eq, eyre, http::Client};

#[tanu::test]
async fn always_fails() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .get(format!("{base_url}/response-headers"))
        .query(&[("status", "400"), ("foo", "bar")])
        .send()
        .await?;

    check_eq!(400, res.status().as_u16());

    let json: Value = res.json().await?;
    check_eq!(
        "baz",
        json["foo"].as_str().unwrap(),
        "Intentional failure for CLI/TUI error state previews (enable with --features fail-test)"
    );
    Ok(())
}
