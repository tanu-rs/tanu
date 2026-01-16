//! # HTTP Client Module
//!
//! Tanu's HTTP client provides a high-performance wrapper around `hyper` with enhanced
//! logging and testing capabilities. Built directly on hyper for minimal overhead and
//! precise control over HTTP operations.
//!
//! ## Request/Response Flow (block diagram)
//!
//! ```text
//! +-------------------+     +-------------------+     +-------------------+
//! | Client            | --> | RequestBuilder    | --> | hyper Client      |
//! | get/post/put/...  |     | headers/body/json |     | request(req)      |
//! +-------------------+     +-------------------+     +-------------------+
//!                                                              |
//!                                                              v
//! +-------------------+     +-------------------+     +-------------------+
//! | Response          | <-- | Log (captured)    | <-- | hyper::Response   |
//! | status/headers/   |     | request + response|     | async response    |
//! | text/json         |     | timing info       |     |                   |
//! +-------------------+     +-------------------+     +-------------------+
//!                                   |
//!                                   v
//!                           +-------------------+
//!                           | Event channel     |
//!                           | publish(Http log) |
//!                           +-------------------+
//! ```
//!
//! ## Key Features
//!
//! - **High Performance**: Built directly on hyper for minimal overhead
//! - **Automatic Logging**: Captures all HTTP requests and responses
//! - **Precise Control**: Direct access to hyper's low-level HTTP functionality
//! - **Integration with Assertions**: Works seamlessly with tanu's assertion macros
//! - **Error Handling**: Enhanced error types with context for better debugging
//!
//! ## Basic Usage
//!
//! ```rust,ignore
//! use tanu::{check_eq, http::Client};
//!
//! #[tanu::test]
//! async fn test_api() -> eyre::Result<()> {
//!     let client = Client::new();
//!
//!     let response = client
//!         .get("https://api.example.com/users")
//!         .header("accept", "application/json")
//!         .send()
//!         .await?;
//!
//!     check_eq!(200, response.status().as_u16());
//!
//!     let users: serde_json::Value = response.json().await?;
//!     check!(users.is_array());
//!
//!     Ok(())
//! }
//! ```
use bytes::Bytes;
pub use http::{header, Method, StatusCode, Version};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::Request;
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::rt::TokioExecutor;
use std::io::Read;
use std::time::{Duration, Instant, SystemTime};
use tracing::*;

#[cfg(feature = "cookies")]
use std::collections::HashMap;

/// A trait to convert various types into URL strings.
/// This provides API compatibility for URL handling.
pub trait IntoUrl {
    fn into_url_string(self) -> String;
}

impl IntoUrl for &str {
    fn into_url_string(self) -> String {
        self.to_string()
    }
}

impl IntoUrl for String {
    fn into_url_string(self) -> String {
        self
    }
}

impl IntoUrl for &String {
    fn into_url_string(self) -> String {
        self.clone()
    }
}

impl IntoUrl for url::Url {
    fn into_url_string(self) -> String {
        self.to_string()
    }
}

impl IntoUrl for &url::Url {
    fn into_url_string(self) -> String {
        self.to_string()
    }
}

#[cfg(feature = "multipart")]
#[derive(Debug)]
pub struct MultipartForm {
    // Placeholder for multipart form data
    // Full implementation would require additional work
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HttpError: {0}")]
    Http(#[from] hyper::Error),
    #[error("HttpError: {0}")]
    HttpLegacy(#[from] hyper_util::client::legacy::Error),
    #[error("UriError: {0}")]
    Uri(#[from] http::uri::InvalidUri),
    #[error("HeaderError: {0}")]
    Header(#[from] http::Error),
    #[error("TlsError: {0}")]
    Tls(#[from] hyper_tls::native_tls::Error),
    #[error("Request timed out after {0:?}")]
    Timeout(Duration),
    #[error("failed to deserialize http response into the specified type: {0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("{0:#}")]
    Unexpected(#[from] eyre::Error),
}

#[derive(Debug, Clone)]
pub struct LogRequest {
    pub url: url::Url,
    pub method: Method,
    pub headers: header::HeaderMap,
}

#[derive(Debug, Clone, Default)]
pub struct LogResponse {
    pub headers: header::HeaderMap,
    pub body: String,
    pub status: StatusCode,
    pub duration_req: Duration,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub request: LogRequest,
    pub response: LogResponse,
    pub started_at: SystemTime,
    pub ended_at: SystemTime,
}

/// HTTP response wrapper with enhanced testing capabilities.
///
/// This struct wraps HTTP response data and provides convenient methods
/// for accessing response information in tests. All data is captured
/// for logging and debugging purposes.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu::{check_eq, http::Client};
///
/// #[tanu::test]
/// async fn test_response() -> eyre::Result<()> {
///     let client = Client::new();
///     let response = client.get("https://api.example.com").send().await?;
///
///     // Check status
///     check_eq!(200, response.status().as_u16());
///
///     // Access headers
///     let content_type = response.headers().get("content-type");
///
///     // Parse JSON
///     let data: serde_json::Value = response.json().await?;
///
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Response {
    pub headers: header::HeaderMap,
    pub status: StatusCode,
    pub text: String,
    pub url: url::Url,
    #[cfg(feature = "cookies")]
    cookies: Vec<cookie::Cookie<'static>>,
}

impl Response {
    /// Returns the HTTP status code of the response.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let status = response.status();
    /// check_eq!(200, status.as_u16());
    /// check!(status.is_success());
    /// ```
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns a reference to the response headers.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let headers = response.headers();
    /// let content_type = headers.get("content-type").unwrap();
    /// check_str_eq!("application/json", content_type.to_str().unwrap());
    /// ```
    pub fn headers(&self) -> &header::HeaderMap {
        &self.headers
    }

    /// Returns the final URL of the response, after following redirects.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let url = response.url();
    /// check!(url.host_str().unwrap().contains("example.com"));
    /// ```
    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// Consumes the response and returns the response body as a string.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let body = response.text().await?;
    /// check!(body.contains("expected content"));
    /// ```
    pub async fn text(self) -> Result<String, Error> {
        Ok(self.text)
    }

    /// Consumes the response and deserializes the JSON body into the given type.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Parse as serde_json::Value
    /// let data: serde_json::Value = response.json().await?;
    /// check_eq!("John", data["name"]);
    ///
    /// // Parse into custom struct
    /// #[derive(serde::Deserialize)]
    /// struct User { name: String, id: u64 }
    /// let user: User = response.json().await?;
    /// check_eq!("John", user.name);
    /// ```
    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, Error> {
        Ok(serde_json::from_str(&self.text)?)
    }

    #[cfg(feature = "cookies")]
    pub fn cookies(&self) -> impl Iterator<Item = &cookie::Cookie<'static>> + '_ {
        self.cookies.iter()
    }

    async fn from(res: hyper::Response<Incoming>, url: url::Url) -> Result<Self, Error> {
        let headers = res.headers().clone();
        let status = res.status();

        #[cfg(feature = "cookies")]
        let cookies: Vec<cookie::Cookie<'static>> = headers
            .get_all("set-cookie")
            .iter()
            .filter_map(|cookie_header| {
                cookie_header.to_str().ok().and_then(|cookie_str| {
                    cookie::Cookie::parse(cookie_str)
                        .ok()
                        .map(|c| c.into_owned())
                })
            })
            .collect();

        let body_bytes = res.into_body().collect().await?.to_bytes();

        // Handle content decompression
        let text = Self::decompress_body(&headers, &body_bytes);

        Ok(Response {
            headers,
            status,
            url,
            text,
            #[cfg(feature = "cookies")]
            cookies,
        })
    }

    fn decompress_body(headers: &header::HeaderMap, body_bytes: &Bytes) -> String {
        match headers
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
        {
            Some("gzip") => {
                use flate2::read::GzDecoder;
                let mut decoder = GzDecoder::new(body_bytes.as_ref());
                let mut decompressed = Vec::new();
                match decoder.read_to_end(&mut decompressed) {
                    Ok(_) => String::from_utf8_lossy(&decompressed).to_string(),
                    Err(_) => String::from_utf8_lossy(body_bytes).to_string(),
                }
            }
            Some("deflate") => {
                use flate2::read::{DeflateDecoder, ZlibDecoder};

                // Try zlib format first (most common for HTTP deflate)
                let mut zlib_decoder = ZlibDecoder::new(body_bytes.as_ref());
                let mut decompressed = Vec::new();
                match zlib_decoder.read_to_end(&mut decompressed) {
                    Ok(_) => String::from_utf8_lossy(&decompressed).to_string(),
                    Err(_) => {
                        // Fallback to raw deflate format
                        let mut deflate_decoder = DeflateDecoder::new(body_bytes.as_ref());
                        let mut decompressed = Vec::new();
                        match deflate_decoder.read_to_end(&mut decompressed) {
                            Ok(_) => String::from_utf8_lossy(&decompressed).to_string(),
                            Err(_) => String::from_utf8_lossy(body_bytes).to_string(),
                        }
                    }
                }
            }
            Some("br") => {
                let mut decompressed = Vec::new();
                match brotli::Decompressor::new(body_bytes.as_ref(), 4096)
                    .read_to_end(&mut decompressed)
                {
                    Ok(_) => String::from_utf8_lossy(&decompressed).to_string(),
                    Err(_) => String::from_utf8_lossy(body_bytes).to_string(),
                }
            }
            Some("zstd") => {
                match zstd::decode_all(body_bytes.as_ref()) {
                    Ok(decompressed) => String::from_utf8_lossy(&decompressed).to_string(),
                    Err(_) => String::from_utf8_lossy(body_bytes).to_string(),
                }
            }
            _ => String::from_utf8_lossy(body_bytes).to_string(),
        }
    }
}

/// Tanu's HTTP client that provides enhanced testing capabilities.
///
/// This client is built on hyper for high performance and precise control
/// while adding automatic request/response logging, better error handling,
/// and integration with tanu's test reporting system.
///
/// # Features
///
/// - **High Performance**: Built on hyper for minimal overhead
/// - **Automatic Logging**: All requests and responses are captured for debugging
/// - **Enhanced Errors**: Detailed error context for better test debugging
/// - **Cookie Support**: Optional cookie handling with the `cookies` feature
///
/// # Examples
///
/// ```rust,ignore
/// use tanu::{check, http::Client};
///
/// #[tanu::test]
/// async fn test_api() -> eyre::Result<()> {
///     let client = Client::new();
///
///     let response = client
///         .get("https://api.example.com/health")
///         .send()
///         .await?;
///
///     check!(response.status().is_success());
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Client {
    pub(crate) inner: HyperClient<
        hyper_tls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Full<Bytes>,
    >,
    #[cfg(feature = "cookies")]
    pub(crate) cookie_store:
        std::sync::Arc<tokio::sync::RwLock<HashMap<String, Vec<cookie::Cookie<'static>>>>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a new HTTP client instance.
    ///
    /// This creates a client with default settings, including cookie support
    /// if the `cookies` feature is enabled. The client is configured for
    /// optimal testing performance and reliability.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu::http::Client;
    ///
    /// let client = Client::new();
    /// ```
    pub fn new() -> Client {
        let https = hyper_tls::HttpsConnector::new();
        let inner = HyperClient::builder(TokioExecutor::new()).build::<_, Full<Bytes>>(https);

        Client {
            inner,
            #[cfg(feature = "cookies")]
            cookie_store: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn get<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        let url_str = url.into_url_string();
        debug!("Requesting {url_str}");
        RequestBuilder::new(self.clone(), Method::GET, &url_str)
    }

    pub fn post<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        let url_str = url.into_url_string();
        debug!("Requesting {url_str}");
        RequestBuilder::new(self.clone(), Method::POST, &url_str)
    }

    pub fn put<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        let url_str = url.into_url_string();
        debug!("Requesting {url_str}");
        RequestBuilder::new(self.clone(), Method::PUT, &url_str)
    }

    pub fn patch<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        let url_str = url.into_url_string();
        debug!("Requesting {url_str}");
        RequestBuilder::new(self.clone(), Method::PATCH, &url_str)
    }

    pub fn delete<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        let url_str = url.into_url_string();
        debug!("Requesting {url_str}");
        RequestBuilder::new(self.clone(), Method::DELETE, &url_str)
    }

    pub fn head<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        let url_str = url.into_url_string();
        debug!("Requesting {url_str}");
        RequestBuilder::new(self.clone(), Method::HEAD, &url_str)
    }
}

pub struct RequestBuilder {
    client: Client,
    method: Method,
    url: String,
    headers: header::HeaderMap,
    body: Option<Vec<u8>>,
    query_params: Vec<(String, String)>,
    timeout: Option<Duration>,
}

impl RequestBuilder {
    fn new(client: Client, method: Method, url: &str) -> Self {
        Self {
            client,
            method,
            url: url.to_string(),
            headers: header::HeaderMap::new(),
            body: None,
            query_params: Vec::new(),
            timeout: None,
        }
    }

    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        header::HeaderName: TryFrom<K>,
        <header::HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        header::HeaderValue: TryFrom<V>,
        <header::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        if let (Ok(name), Ok(val)) = (
            header::HeaderName::try_from(key),
            header::HeaderValue::try_from(value),
        ) {
            self.headers.insert(name, val);
        }
        self
    }

    pub fn headers(mut self, headers: header::HeaderMap) -> Self {
        self.headers.extend(headers);
        self
    }

    pub fn basic_auth<U, P>(mut self, username: U, password: Option<P>) -> Self
    where
        U: std::fmt::Display,
        P: std::fmt::Display,
    {
        let auth_value = match password {
            Some(p) => format!("{username}:{p}"),
            None => username.to_string(),
        };
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            auth_value.as_bytes(),
        );
        let auth_header = format!("Basic {encoded}");

        if let Ok(header_value) = header::HeaderValue::from_str(&auth_header) {
            self.headers.insert(header::AUTHORIZATION, header_value);
        }
        self
    }

    pub fn bearer_auth<T>(mut self, token: T) -> Self
    where
        T: std::fmt::Display,
    {
        let auth_header = format!("Bearer {token}");
        if let Ok(header_value) = header::HeaderValue::from_str(&auth_header) {
            self.headers.insert(header::AUTHORIZATION, header_value);
        }
        self
    }

    pub fn body<T: Into<Vec<u8>>>(mut self, body: T) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        if let Ok(params) = serde_urlencoded::to_string(query) {
            for pair in params.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    self.query_params.push((key.to_string(), value.to_string()));
                }
            }
        }
        self
    }

    pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> Self {
        if let Ok(body) = serde_urlencoded::to_string(form) {
            self.body = Some(body.into_bytes());
            self.headers.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
        }
        self
    }

    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> Self {
        if let Ok(body) = serde_json::to_string(json) {
            self.body = Some(body.into_bytes());
            self.headers.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            );
        }
        self
    }

    #[cfg(feature = "multipart")]
    pub fn multipart(self, _multipart: MultipartForm) -> Self {
        // Note: Multipart support would need additional implementation
        // For now, this is a placeholder to maintain API compatibility
        self
    }

    pub async fn send(self) -> Result<Response, Error> {
        let mut url = self.url.clone();

        // Add query parameters
        if !self.query_params.is_empty() {
            let query_string: String = self
                .query_params
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&");

            url = if url.contains('?') {
                format!("{url}&{query_string}")
            } else {
                format!("{url}?{query_string}")
            };
        }

        let parsed_url = url::Url::parse(&url).map_err(|e| eyre::eyre!("Invalid URL: {}", e))?;
        let uri: http::Uri = url.parse()?;

        let mut req_builder = Request::builder().method(self.method.clone()).uri(uri);

        // Add headers
        for (name, value) in &self.headers {
            req_builder = req_builder.header(name, value);
        }

        #[cfg(feature = "cookies")]
        {
            // Add cookies for this domain
            let cookie_store = self.client.cookie_store.read().await;
            if let Some(domain_cookies) = cookie_store.get(parsed_url.host_str().unwrap_or("")) {
                if !domain_cookies.is_empty() {
                    let cookie_header = domain_cookies
                        .iter()
                        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                        .collect::<Vec<_>>()
                        .join("; ");

                    if let Ok(cookie_value) = header::HeaderValue::from_str(&cookie_header) {
                        req_builder = req_builder.header(header::COOKIE, cookie_value);
                    }
                }
            }
        }

        let body = match &self.body {
            Some(ref body_data) => Full::new(Bytes::from(body_data.clone())),
            None => Full::new(Bytes::new()),
        };

        let req = req_builder.body(body)?;

        let log_request = LogRequest {
            url: parsed_url.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
        };

        let started_at = SystemTime::now();
        let time_req = Instant::now();

        // Apply timeout if specified
        let res = match self.timeout {
            Some(timeout) => match tokio::time::timeout(timeout, self.client.inner.request(req)).await {
                Ok(result) => result,
                Err(_) => return Err(Error::Timeout(timeout)),
            },
            None => self.client.inner.request(req).await,
        };
        let ended_at = SystemTime::now();

        match res {
            Ok(res) => {
                let status = res.status();

                // Handle redirects - follow up to 10 redirects
                if status.is_redirection() {
                    return Self::follow_redirects(
                        self.client.clone(),
                        self.headers.clone(),
                        self.method.clone(),
                        self.body.clone(),
                        res,
                        parsed_url,
                        log_request,
                        started_at,
                        time_req,
                        10,
                    )
                    .await;
                }

                let response = Response::from(res, parsed_url).await?;
                let duration_req = time_req.elapsed();

                #[cfg(feature = "cookies")]
                {
                    // Store cookies from response
                    if !response.cookies.is_empty() {
                        let mut cookie_store = self.client.cookie_store.write().await;
                        let domain = response.url().host_str().unwrap_or("").to_string();
                        cookie_store.insert(domain, response.cookies.clone());
                    }
                }

                let log_response = LogResponse {
                    headers: response.headers.clone(),
                    body: response.text.clone(),
                    status: response.status(),
                    duration_req,
                };

                crate::runner::publish(crate::runner::EventBody::Http(Box::new(Log {
                    request: log_request,
                    response: log_response,
                    started_at,
                    ended_at,
                })))?;
                Ok(response)
            }
            Err(e) => {
                crate::runner::publish(crate::runner::EventBody::Http(Box::new(Log {
                    request: log_request,
                    response: Default::default(),
                    started_at,
                    ended_at,
                })))?;
                Err(e.into())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn follow_redirects(
        client: Client,
        headers: header::HeaderMap,
        mut method: Method,
        body: Option<Vec<u8>>,
        mut response: hyper::Response<Incoming>,
        mut current_url: url::Url,
        original_request: LogRequest,
        started_at: SystemTime,
        start_time: Instant,
        max_redirects: u8,
    ) -> Result<Response, Error> {
        let mut redirect_count = 0;

        loop {
            let status = response.status();

            if !status.is_redirection() || redirect_count >= max_redirects {
                let ended_at = SystemTime::now();
                let final_response = Response::from(response, current_url).await?;
                let duration_req = start_time.elapsed();

                #[cfg(feature = "cookies")]
                {
                    if !final_response.cookies.is_empty() {
                        let mut cookie_store = client.cookie_store.write().await;
                        let domain = final_response.url().host_str().unwrap_or("").to_string();
                        cookie_store.insert(domain, final_response.cookies.clone());
                    }
                }

                let log_response = LogResponse {
                    headers: final_response.headers.clone(),
                    body: final_response.text.clone(),
                    status: final_response.status(),
                    duration_req,
                };

                crate::runner::publish(crate::runner::EventBody::Http(Box::new(Log {
                    request: original_request,
                    response: log_response,
                    started_at,
                    ended_at,
                })))?;

                return Ok(final_response);
            }

            // Extract cookies from redirect response
            #[cfg(feature = "cookies")]
            {
                let redirect_cookies: Vec<cookie::Cookie<'static>> = response
                    .headers()
                    .get_all("set-cookie")
                    .iter()
                    .filter_map(|cookie_header| {
                        cookie_header.to_str().ok().and_then(|cookie_str| {
                            cookie::Cookie::parse(cookie_str)
                                .ok()
                                .map(|c| c.into_owned())
                        })
                    })
                    .collect();

                if !redirect_cookies.is_empty() {
                    let mut cookie_store = client.cookie_store.write().await;
                    let domain = current_url.host_str().unwrap_or("").to_string();
                    let existing_cookies =
                        cookie_store.entry(domain.clone()).or_insert_with(Vec::new);
                    existing_cookies.extend(redirect_cookies);
                }
            }

            // Get redirect location
            let location = match response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
            {
                Some(loc) => loc,
                None => {
                    // Some status codes don't require location headers
                    let ended_at = SystemTime::now();
                    let final_response = Response::from(response, current_url).await?;
                    let duration_req = start_time.elapsed();

                    let log_response = LogResponse {
                        headers: final_response.headers.clone(),
                        body: final_response.text.clone(),
                        status: final_response.status(),
                        duration_req,
                    };

                    crate::runner::publish(crate::runner::EventBody::Http(Box::new(Log {
                        request: original_request,
                        response: log_response,
                        started_at,
                        ended_at,
                    })))?;

                    return Ok(final_response);
                }
            };

            // Construct new URL
            current_url = if location.starts_with("http") {
                url::Url::parse(location).map_err(|e| eyre::eyre!("Invalid redirect URL: {}", e))?
            } else {
                current_url
                    .join(location)
                    .map_err(|e| eyre::eyre!("Invalid redirect URL: {}", e))?
            };

            // Update method for redirect (follow HTTP redirect semantics)
            if status == StatusCode::SEE_OTHER
                || (method == Method::POST
                    && (status == StatusCode::MOVED_PERMANENTLY || status == StatusCode::FOUND))
            {
                method = Method::GET;
            }

            // Build redirect request
            let redirect_uri: http::Uri = current_url.to_string().parse()?;
            let mut redirect_req_builder =
                Request::builder().method(method.clone()).uri(redirect_uri);

            // Add original headers
            for (name, value) in &headers {
                redirect_req_builder = redirect_req_builder.header(name, value);
            }

            // Add cookies for new domain
            #[cfg(feature = "cookies")]
            {
                let cookie_store = client.cookie_store.read().await;
                if let Some(domain_cookies) = cookie_store.get(current_url.host_str().unwrap_or(""))
                {
                    if !domain_cookies.is_empty() {
                        let cookie_header = domain_cookies
                            .iter()
                            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                            .collect::<Vec<_>>()
                            .join("; ");

                        if let Ok(cookie_value) = header::HeaderValue::from_str(&cookie_header) {
                            redirect_req_builder =
                                redirect_req_builder.header(header::COOKIE, cookie_value);
                        }
                    }
                }
            }

            let redirect_body = if method == Method::GET {
                Full::new(Bytes::new())
            } else {
                match &body {
                    Some(body_data) => Full::new(Bytes::from(body_data.clone())),
                    None => Full::new(Bytes::new()),
                }
            };

            let redirect_req = redirect_req_builder.body(redirect_body)?;
            response = client.inner.request(redirect_req).await?;
            redirect_count += 1;
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn try_clone(&self) -> Option<Self> {
        Some(Self {
            client: self.client.clone(),
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            query_params: self.query_params.clone(),
            timeout: self.timeout,
        })
    }

    pub fn version(self, _version: Version) -> Self {
        // Note: hyper automatically handles HTTP versions
        // This method is kept for API compatibility
        self
    }
}
