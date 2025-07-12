use tanu::{check, check_eq, eyre, http::Client};

#[tanu::test]
async fn same_test_name_in_different_modules() -> eyre::Result<()> {
    Ok(())
}

// Test with eyre::Result (already shown above)
#[tanu::test]
async fn test_with_eyre_result() -> eyre::Result<()> {
    let client = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let response = client
        .get(format!("{base_url}/get"))
        .query(&[("test", "eyre")])
        .send()
        .await?;

    check!(
        response.status().is_success(),
        "Expected successful response with eyre"
    );

    let json: serde_json::Value = response.json().await?;
    check_eq!("eyre", json["args"]["test"].as_str().unwrap());

    Ok(())
}

// Test with anyhow::Result
#[tanu::test]
async fn test_with_anyhow_result() -> anyhow::Result<()> {
    let client = Client::new();
    let base_url = crate::get_httpbin()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get httpbin: {}", e))?
        .get_base_url()
        .await;

    let response = client
        .get(format!("{base_url}/get"))
        .query(&[("test", "anyhow")])
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

    // Use manual assertions instead of tanu macros for anyhow compatibility
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Expected successful response with anyhow, got: {}",
            response.status()
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

    let test_value = json["args"]["test"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing test parameter in response"))?;

    if test_value != "anyhow" {
        return Err(anyhow::anyhow!("Expected 'anyhow', got '{}'", test_value));
    }

    Ok(())
}

// Test with std::result::Result and custom error
#[derive(Debug)]
enum CustomError {
    Http(String),
    Parse(String),
    Validation(String),
}

impl std::fmt::Display for CustomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CustomError::Http(msg) => write!(f, "HTTP Error: {msg}"),
            CustomError::Parse(msg) => write!(f, "Parse Error: {msg}"),
            CustomError::Validation(msg) => write!(f, "Validation Error: {msg}"),
        }
    }
}

impl std::error::Error for CustomError {}

#[tanu::test]
async fn test_with_custom_result() -> Result<(), CustomError> {
    let client = Client::new();
    let base_url = crate::get_httpbin()
        .await
        .map_err(|e| CustomError::Http(format!("Failed to get httpbin: {e}")))?
        .get_base_url()
        .await;

    let response = client
        .get(format!("{base_url}/get"))
        .query(&[("test", "custom")])
        .send()
        .await
        .map_err(|e| CustomError::Http(format!("Request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(CustomError::Validation(format!(
            "Expected success status, got: {}",
            response.status()
        )));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| CustomError::Parse(format!("JSON parsing failed: {e}")))?;

    let test_value = json["args"]["test"]
        .as_str()
        .ok_or_else(|| CustomError::Validation("Missing test parameter".to_string()))?;

    if test_value != "custom" {
        return Err(CustomError::Validation(format!(
            "Expected 'custom', got '{test_value}'"
        )));
    }

    Ok(())
}

// Test with simple std::result::Result<(), String>
#[tanu::test]
async fn test_with_string_error() -> Result<(), String> {
    let client = Client::new();
    let base_url = crate::get_httpbin()
        .await
        .map_err(|e| format!("Failed to get httpbin: {e}"))?
        .get_base_url()
        .await;

    let response = client
        .get(format!("{base_url}/status/200"))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if response.status().as_u16() != 200 {
        return Err(format!("Expected status 200, got {}", response.status()));
    }

    Ok(())
}

// Test with different parameterized result types
#[tanu::test("eyre")]
#[tanu::test("anyhow")]
#[tanu::test("custom")]
async fn test_result_types_parameterized(test_type: &str) -> eyre::Result<()> {
    let client = Client::new();
    let base_url = crate::get_httpbin().await?.get_base_url().await;

    let response = client
        .get(format!("{base_url}/get"))
        .query(&[("result_type", test_type)])
        .send()
        .await?;

    check!(response.status().is_success());

    let json: serde_json::Value = response.json().await?;
    check_eq!(test_type, json["args"]["result_type"].as_str().unwrap());

    Ok(())
}

// Test demonstrating that tanu can handle simple Result types
#[derive(Debug)]
struct SimpleError;

impl std::fmt::Display for SimpleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Simple error occurred")
    }
}

impl std::error::Error for SimpleError {}

#[tanu::test]
async fn test_with_simple_result() -> Result<(), SimpleError> {
    let client = Client::new();
    let base_url = crate::get_httpbin()
        .await
        .map_err(|_| SimpleError)?
        .get_base_url()
        .await;

    let response = client
        .get(format!("{base_url}/status/200"))
        .send()
        .await
        .map_err(|_| SimpleError)?;

    if !response.status().is_success() {
        return Err(SimpleError);
    }

    Ok(())
}
