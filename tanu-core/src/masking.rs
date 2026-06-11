//! Sensitive data masking utilities for HTTP logging.
//!
//! This module provides utilities for masking sensitive data in HTTP logs,
//! such as API keys in query strings and authorization headers.

use http::header::{HeaderMap, HeaderValue};
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;

/// The mask string used to replace sensitive values.
const MASK: &str = "*****";

/// Global flag for masking (set by Runner at startup).
static MASK_SENSITIVE: AtomicBool = AtomicBool::new(true);

/// Sets whether sensitive data should be masked.
///
/// This is called by the Runner at startup based on CLI options.
pub fn set_mask_sensitive(enabled: bool) {
    MASK_SENSITIVE.store(enabled, Ordering::Relaxed);
}

/// Returns whether sensitive data should be masked.
pub fn should_mask_sensitive() -> bool {
    MASK_SENSITIVE.load(Ordering::Relaxed)
}

/// Query parameter names to mask (case-insensitive comparison).
const SENSITIVE_QUERY_PARAMS: &[&str] = &[
    "access_token",
    "api_key",
    "apikey",
    "token",
    "secret",
    "password",
    "key",
    "auth",
];

/// Header names to mask (case-insensitive comparison).
const SENSITIVE_HEADERS: &[&str] = &["authorization", "x-api-key", "x-auth-token", "cookie"];

/// Returns true if the given key (field/query-param name) is sensitive.
fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    SENSITIVE_QUERY_PARAMS.iter().any(|&p| key_lower == p)
}

/// Masks sensitive query parameters in a URL.
///
/// # Examples
///
/// ```
/// use url::Url;
/// use tanu_core::masking::mask_url;
///
/// let url = Url::parse("https://api.example.com/users?access_token=secret123&name=john").unwrap();
/// let masked = mask_url(&url);
/// assert!(masked.to_string().contains("access_token=*****"));
/// assert!(masked.to_string().contains("name=john"));
/// ```
pub fn mask_url(url: &Url) -> Url {
    let mut masked_url = url.clone();

    // Check if there are any query parameters
    let Some(query) = url.query() else {
        return masked_url;
    };

    // Work with raw query string to preserve original encoding
    let masked_query = query
        .split('&')
        .map(|pair| {
            // Split on first '=' only to preserve encoded '=' in values
            if let Some((key, _value)) = pair.split_once('=') {
                if is_sensitive_key(key) {
                    format!("{key}={MASK}")
                } else {
                    pair.to_string()
                }
            } else {
                // Parameter without value (e.g., ?flag)
                pair.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("&");

    masked_url.set_query(Some(&masked_query));
    masked_url
}

/// Masks sensitive header values in a HeaderMap.
///
/// # Examples
///
/// ```
/// use http::header::HeaderMap;
/// use tanu_core::masking::mask_headers;
///
/// let mut headers = HeaderMap::new();
/// headers.insert("authorization", "Bearer secret".parse().unwrap());
/// headers.insert("content-type", "application/json".parse().unwrap());
///
/// let masked = mask_headers(&headers);
/// assert_eq!(masked.get("authorization").unwrap(), "*****");
/// assert_eq!(masked.get("content-type").unwrap(), "application/json");
/// ```
pub fn mask_headers(headers: &HeaderMap) -> HeaderMap {
    let mut masked = HeaderMap::new();

    for (name, value) in headers.iter() {
        let name_lower = name.as_str().to_lowercase();
        let masked_value = if SENSITIVE_HEADERS.iter().any(|&h| name_lower == h) {
            HeaderValue::from_static(MASK)
        } else {
            value.clone()
        };
        masked.insert(name.clone(), masked_value);
    }

    masked
}

/// Masks sensitive field values in an HTTP request body.
///
/// Behavior depends on the content-type:
/// - `application/json*`: parses as JSON and recursively redacts values whose
///   keys match the sensitive-key list. Falls back to raw UTF-8 on parse failure.
/// - `application/x-www-form-urlencoded`: masks values for sensitive keys.
/// - Everything else: returned verbatim as lossy UTF-8.
///
/// # Examples
///
/// ```
/// use tanu_core::masking::mask_body;
///
/// let body = br#"{"username":"alice","password":"hunter2"}"#;
/// let masked = mask_body(body, Some("application/json"));
/// assert!(masked.contains("\"password\":\"*****\""));
/// assert!(masked.contains("\"username\":\"alice\""));
/// ```
pub fn mask_body(body: &[u8], content_type: Option<&str>) -> String {
    let ct = content_type.unwrap_or("").to_lowercase();

    if ct.starts_with("application/json") {
        match serde_json::from_slice::<serde_json::Value>(body) {
            Ok(json) => {
                let masked_json = mask_json_value(json);
                // Use compact serialization to match original formatting
                serde_json::to_string(&masked_json)
                    .unwrap_or_else(|_| String::from_utf8_lossy(body).into_owned())
            }
            Err(_) => String::from_utf8_lossy(body).into_owned(),
        }
    } else if ct.starts_with("application/x-www-form-urlencoded") {
        let raw = String::from_utf8_lossy(body);
        raw.split('&')
            .map(|pair| {
                if let Some((key, _value)) = pair.split_once('=') {
                    if is_sensitive_key(key) {
                        format!("{key}={MASK}")
                    } else {
                        pair.to_string()
                    }
                } else {
                    pair.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("&")
    } else {
        String::from_utf8_lossy(body).into_owned()
    }
}

/// Recursively walks a JSON value, masking sensitive object field values.
fn mask_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let masked_map = map
                .into_iter()
                .map(|(k, v)| {
                    let new_v = if is_sensitive_key(&k) {
                        serde_json::Value::String(MASK.to_string())
                    } else {
                        mask_json_value(v)
                    };
                    (k, new_v)
                })
                .collect();
            serde_json::Value::Object(masked_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(mask_json_value).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_url_with_sensitive_params() {
        let url =
            Url::parse("https://api.example.com/users?access_token=secret123&name=john").unwrap();
        let masked = mask_url(&url);
        assert!(masked.to_string().contains("access_token=*****"));
        assert!(masked.to_string().contains("name=john"));
    }

    #[test]
    fn test_mask_url_multiple_sensitive_params() {
        let url = Url::parse("https://api.example.com/data?api_key=key123&token=tok456&user=alice")
            .unwrap();
        let masked = mask_url(&url);
        assert!(masked.to_string().contains("api_key=*****"));
        assert!(masked.to_string().contains("token=*****"));
        assert!(masked.to_string().contains("user=alice"));
    }

    #[test]
    fn test_mask_url_case_insensitive() {
        let url = Url::parse("https://api.example.com/?ACCESS_TOKEN=secret&API_KEY=key").unwrap();
        let masked = mask_url(&url);
        assert!(masked.to_string().contains("ACCESS_TOKEN=*****"));
        assert!(masked.to_string().contains("API_KEY=*****"));
    }

    #[test]
    fn test_mask_url_without_query_params() {
        let url = Url::parse("https://api.example.com/users").unwrap();
        let masked = mask_url(&url);
        assert_eq!(url.to_string(), masked.to_string());
    }

    #[test]
    fn test_mask_url_no_sensitive_params() {
        let url = Url::parse("https://api.example.com/users?page=1&limit=10").unwrap();
        let masked = mask_url(&url);
        assert!(masked.to_string().contains("page=1"));
        assert!(masked.to_string().contains("limit=10"));
    }

    #[test]
    fn test_mask_url_preserves_encoding() {
        let url =
            Url::parse("https://api.example.com/users?access_token=secret%2Btoken&name=john%20doe")
                .unwrap();
        let masked = mask_url(&url);
        let masked_str = masked.to_string();
        assert!(masked_str.contains("access_token=*****"));
        assert!(masked_str.contains("name=john%20doe"));
    }

    #[test]
    fn test_mask_url_repeated_keys() {
        let url =
            Url::parse("https://api.example.com/users?token=one&token=two&user=alice").unwrap();
        let masked = mask_url(&url);
        let masked_str = masked.to_string();
        assert!(masked_str.contains("token=*****&token=*****"));
        assert!(masked_str.contains("user=alice"));
    }

    #[test]
    fn test_mask_url_empty_sensitive_value() {
        let url = Url::parse("https://api.example.com/users?access_token=").unwrap();
        let masked = mask_url(&url);
        assert!(masked.to_string().contains("access_token=*****"));
    }

    #[test]
    fn test_mask_headers_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer secret".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let masked = mask_headers(&headers);
        assert_eq!(masked.get("authorization").unwrap(), "*****");
        assert_eq!(masked.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn test_mask_headers_multiple_sensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token".parse().unwrap());
        headers.insert("x-api-key", "apikey123".parse().unwrap());
        headers.insert("cookie", "session=abc".parse().unwrap());
        headers.insert("accept", "application/json".parse().unwrap());

        let masked = mask_headers(&headers);
        assert_eq!(masked.get("authorization").unwrap(), "*****");
        assert_eq!(masked.get("x-api-key").unwrap(), "*****");
        assert_eq!(masked.get("cookie").unwrap(), "*****");
        assert_eq!(masked.get("accept").unwrap(), "application/json");
    }

    #[test]
    fn test_mask_headers_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer token".parse().unwrap());
        headers.insert("X-API-Key", "apikey123".parse().unwrap());

        let masked = mask_headers(&headers);
        assert_eq!(masked.get("authorization").unwrap(), "*****");
        assert_eq!(masked.get("x-api-key").unwrap(), "*****");
    }

    #[test]
    fn test_mask_headers_empty() {
        let headers = HeaderMap::new();
        let masked = mask_headers(&headers);
        assert!(masked.is_empty());
    }

    #[test]
    fn test_mask_headers_no_sensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("accept", "text/html".parse().unwrap());

        let masked = mask_headers(&headers);
        assert_eq!(masked.get("content-type").unwrap(), "application/json");
        assert_eq!(masked.get("accept").unwrap(), "text/html");
    }

    #[test]
    fn test_should_mask_sensitive_default() {
        // Reset to default
        set_mask_sensitive(true);
        assert!(should_mask_sensitive());
    }

    #[test]
    fn test_set_mask_sensitive() {
        set_mask_sensitive(false);
        assert!(!should_mask_sensitive());

        set_mask_sensitive(true);
        assert!(should_mask_sensitive());
    }

    #[test]
    fn test_mask_body_json_sensitive_keys() {
        let body = br#"{"username":"alice","password":"hunter2","token":"abc123"}"#;
        let masked = mask_body(body, Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(&masked).unwrap();
        assert_eq!(v["username"], "alice");
        assert_eq!(v["password"], "*****");
        assert_eq!(v["token"], "*****");
    }

    #[test]
    fn test_mask_body_json_no_sensitive_keys() {
        let body = br#"{"name":"alice","age":30}"#;
        let masked = mask_body(body, Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(&masked).unwrap();
        assert_eq!(v["name"], "alice");
        assert_eq!(v["age"], 30);
    }

    #[test]
    fn test_mask_body_json_nested() {
        let body = br#"{"user":{"password":"s3cr3t","name":"bob"}}"#;
        let masked = mask_body(body, Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(&masked).unwrap();
        assert_eq!(v["user"]["password"], "*****");
        assert_eq!(v["user"]["name"], "bob");
    }

    #[test]
    fn test_mask_body_json_case_insensitive_key() {
        let body = br#"{"Password":"secret123"}"#;
        let masked = mask_body(body, Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(&masked).unwrap();
        assert_eq!(v["Password"], "*****");
    }

    #[test]
    fn test_mask_body_json_content_type_with_charset() {
        let body = br#"{"password":"s3cr3t"}"#;
        let masked = mask_body(body, Some("application/json; charset=utf-8"));
        assert!(masked.contains("\"*****\""));
    }

    #[test]
    fn test_mask_body_json_invalid_falls_back_to_raw() {
        let body = b"not valid json";
        let masked = mask_body(body, Some("application/json"));
        assert_eq!(masked, "not valid json");
    }

    #[test]
    fn test_mask_body_form_urlencoded_sensitive() {
        let body = b"username=alice&password=hunter2&token=abc";
        let masked = mask_body(body, Some("application/x-www-form-urlencoded"));
        assert!(masked.contains("username=alice"));
        assert!(masked.contains("password=*****"));
        assert!(masked.contains("token=*****"));
    }

    #[test]
    fn test_mask_body_form_urlencoded_no_sensitive() {
        let body = b"name=alice&age=30";
        let masked = mask_body(body, Some("application/x-www-form-urlencoded"));
        assert_eq!(masked, "name=alice&age=30");
    }

    #[test]
    fn test_mask_body_plain_text_passthrough() {
        let body = b"hello world";
        let masked = mask_body(body, Some("text/plain"));
        assert_eq!(masked, "hello world");
    }

    #[test]
    fn test_mask_body_no_content_type_passthrough() {
        let body = b"some raw bytes";
        let masked = mask_body(body, None);
        assert_eq!(masked, "some raw bytes");
    }
}
