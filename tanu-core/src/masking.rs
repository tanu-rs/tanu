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
                let key_lower = key.to_lowercase();
                if SENSITIVE_QUERY_PARAMS.iter().any(|&p| key_lower == p) {
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
}
