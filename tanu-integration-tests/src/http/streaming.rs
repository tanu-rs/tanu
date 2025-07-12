use tanu::{check, check_eq, eyre, http::Client};

//#[tanu::test]
//async fn stream_bytes() -> eyre::Result<()> {
//    let http = Client::new();
//    let base_url = crate::get_httpbin().await?.get_base_url().await;
//
//    let res = http
//        .get(format!("{base_url}/stream-bytes/1024"))
//        .send()
//        .await?;
//
//    check!(res.status().is_success(), "Non 2xx status received");
//
//    let text = res.text().await?;
//    check_eq!(1024, text.len());
//
//    Ok(())
//}

#[tanu::test]
async fn stream_json() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/stream/5")).send().await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let body = res.text().await?;
    let lines: Vec<&str> = body.lines().collect();

    check_eq!(5, lines.len());

    for line in lines {
        check!(line.contains("\"id\""));
        check!(line.contains("\"url\""));
    }

    Ok(())
}

#[tanu::test]
async fn range_request() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http
        .get(format!("{base_url}/range/1024"))
        .header("Range", "bytes=0-511")
        .send()
        .await?;

    check_eq!(206, res.status().as_u16());

    let text = res.text().await?;
    check_eq!(512, text.len());

    Ok(())
}

#[tanu::test]
async fn drip_endpoint() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http
        .get(format!("{base_url}/drip"))
        .query(&[("numbytes", "100"), ("duration", "1")])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let text = res.text().await?;
    check_eq!(100, text.len());

    Ok(())
}
