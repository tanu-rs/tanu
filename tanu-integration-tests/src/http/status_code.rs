use tanu::{
    check_eq, eyre,
    http::{Client, StatusCode},
};

#[tanu::test]
async fn status_200_ok() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/200")).send().await?;

    check_eq!(StatusCode::OK, res.status());

    Ok(())
}

#[tanu::test]
async fn status_201_created() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/201")).send().await?;

    check_eq!(StatusCode::CREATED, res.status());

    Ok(())
}

#[tanu::test]
async fn status_204_no_content() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/204")).send().await?;

    check_eq!(StatusCode::NO_CONTENT, res.status());

    let body = res.text().await?;
    check_eq!("", body, "204 No Content should have empty body");

    Ok(())
}

#[tanu::test]
async fn status_400_bad_request() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/400")).send().await?;

    check_eq!(StatusCode::BAD_REQUEST, res.status());

    Ok(())
}

#[tanu::test]
async fn status_401_unauthorized() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/401")).send().await?;

    check_eq!(StatusCode::UNAUTHORIZED, res.status());

    Ok(())
}

#[tanu::test]
async fn status_403_forbidden() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/403")).send().await?;

    check_eq!(StatusCode::FORBIDDEN, res.status());

    Ok(())
}

#[tanu::test]
async fn status_404_not_found() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/404")).send().await?;

    check_eq!(StatusCode::NOT_FOUND, res.status());

    Ok(())
}

#[tanu::test]
async fn status_500_internal_server_error() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/500")).send().await?;

    check_eq!(StatusCode::INTERNAL_SERVER_ERROR, res.status());

    Ok(())
}

#[tanu::test]
async fn status_502_bad_gateway() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/502")).send().await?;

    check_eq!(StatusCode::BAD_GATEWAY, res.status());

    Ok(())
}

#[tanu::test]
async fn status_503_service_unavailable() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let res = http.get(format!("{base_url}/status/503")).send().await?;

    check_eq!(StatusCode::SERVICE_UNAVAILABLE, res.status());

    Ok(())
}

#[tanu::test]
async fn random_status_codes() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let test_codes = [200, 201, 300, 400, 401, 500, 502];

    for code in test_codes {
        let res = http.get(format!("{base_url}/status/{code}")).send().await?;

        check_eq!(code, res.status().as_u16());
    }

    Ok(())
}
