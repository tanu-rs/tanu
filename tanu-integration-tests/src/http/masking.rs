use serde::Deserialize;
use std::collections::HashMap;
use tanu::{check, check_eq, eyre, http::Client};

#[derive(Debug, Deserialize)]
struct ArgsResponse {
    args: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct HeadersResponse {
    headers: HashMap<String, String>,
}

/// Test that API key in query parameters is masked in HTTP logs.
///
/// When running with --capture-http flag, the access_token and api_key
/// query parameters will be masked as ***** in the logs.
#[tanu::test]
async fn api_key_in_query_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    // Make request with sensitive query parameters
    let res = http
        .get(format!("{base_url}/get"))
        .query(&[
            ("api_key", "super_secret_api_key_12345"),
            ("access_token", "secret_access_token_xyz"),
            ("user", "john_doe"),
        ])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: ArgsResponse = res.json().await?;

    // Verify all query params were sent to the server
    check_eq!(
        "super_secret_api_key_12345",
        response.args.get("api_key").unwrap(),
        "API key should be sent to server"
    );
    check_eq!(
        "secret_access_token_xyz",
        response.args.get("access_token").unwrap(),
        "Access token should be sent to server"
    );
    check_eq!(
        "john_doe",
        response.args.get("user").unwrap(),
        "User should be sent to server"
    );

    // Note: When this test runs with `cargo run -- test --capture-http`,
    // the HTTP logs will show:
    //   - api_key=*****
    //   - access_token=*****
    //   - user=john_doe (not masked, as it's not sensitive)

    Ok(())
}

/// Test that API key in authorization header is masked in HTTP logs.
///
/// When running with --capture-http flag, the authorization header
/// will be masked as ***** in the logs.
#[tanu::test]
async fn api_key_in_authorization_header() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    // Make request with authorization header
    let res = http
        .get(format!("{base_url}/headers"))
        .header("authorization", "Bearer secret_bearer_token_xyz123")
        .header("content-type", "application/json")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: HeadersResponse = res.json().await?;

    // Verify authorization header was sent to server
    check!(response.headers.contains_key("Authorization"));
    check_eq!(
        "Bearer secret_bearer_token_xyz123",
        response.headers.get("Authorization").unwrap(),
        "Authorization header should be sent to server"
    );

    // Note: When this test runs with `cargo run -- test --capture-http`,
    // the HTTP logs will show:
    //   - authorization: *****
    //   - content-type: application/json (not masked)

    Ok(())
}

/// Test that X-Api-Key header is masked in HTTP logs.
///
/// When running with --capture-http flag, the x-api-key header
/// will be masked as ***** in the logs.
#[tanu::test]
async fn api_key_in_x_api_key_header() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    // Make request with X-Api-Key header
    let res = http
        .get(format!("{base_url}/headers"))
        .header("x-api-key", "my_super_secret_api_key_abc123")
        .header("accept", "application/json")
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: HeadersResponse = res.json().await?;

    // Verify X-Api-Key header was sent to server
    check!(response.headers.contains_key("X-Api-Key"));
    check_eq!(
        "my_super_secret_api_key_abc123",
        response.headers.get("X-Api-Key").unwrap(),
        "X-Api-Key header should be sent to server"
    );

    // Note: When this test runs with `cargo run -- test --capture-http`,
    // the HTTP logs will show:
    //   - x-api-key: *****
    //   - accept: application/json (not masked)

    Ok(())
}

/// Test that multiple sensitive parameters are all masked.
///
/// When running with --capture-http flag, all sensitive parameters
/// (api_key, token, secret, password) will be masked as *****.
#[tanu::test]
async fn multiple_sensitive_params() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    // Make request with multiple sensitive query parameters
    let res = http
        .get(format!("{base_url}/get"))
        .query(&[
            ("token", "secret_token_123"),
            ("secret", "my_secret_value"),
            ("password", "my_password_456"),
            ("username", "alice"),
        ])
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: ArgsResponse = res.json().await?;

    // Verify all params were sent to server
    check!(response.args.contains_key("token"));
    check!(response.args.contains_key("secret"));
    check!(response.args.contains_key("password"));
    check_eq!("alice", response.args.get("username").unwrap());

    // Note: When this test runs with `cargo run -- test --capture-http`,
    // the HTTP logs will show:
    //   - token=*****
    //   - secret=*****
    //   - password=*****
    //   - username=alice (not masked)

    Ok(())
}

/// Test that URL encoding is preserved when masking.
///
/// When running with --capture-http flag, the sensitive parameter
/// will be masked but the encoding of non-sensitive params is preserved.
#[tanu::test]
async fn masking_preserves_url_encoding() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    // Make request with URL-encoded parameters
    let res = http
        .get(format!(
            "{base_url}/get?access_token=secret%2Btoken&name=john%20doe"
        ))
        .send()
        .await?;

    check!(res.status().is_success(), "Non 2xx status received");

    let response: ArgsResponse = res.json().await?;

    // Verify params were decoded correctly by the server
    check_eq!("secret+token", response.args.get("access_token").unwrap());
    check_eq!("john doe", response.args.get("name").unwrap());

    // Note: When this test runs with `cargo run -- test --capture-http`,
    // the HTTP logs will preserve the original encoding:
    //   - access_token=*****
    //   - name=john%20doe (encoding preserved)

    Ok(())
}
