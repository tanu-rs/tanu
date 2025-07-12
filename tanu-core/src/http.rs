/// tanu's HTTP client is a wrapper for `reqwest::Client` and offers * exactly same interface as `reqwest::Client`
/// * to capture reqnest and response logs
use eyre::OptionExt;
pub use http::{header, Method, StatusCode, Version};
use std::time::{Duration, Instant};
use tracing::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HttpError: {0}")]
    Http(#[from] reqwest::Error),
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
}

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
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &header::HeaderMap {
        &self.headers
    }

    pub fn url(&self) -> &url::Url {
        &self.url
    }

    pub async fn text(self) -> Result<String, Error> {
        Ok(self.text)
    }

    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, Error> {
        Ok(serde_json::from_str(&self.text)?)
    }

    #[cfg(feature = "cookies")]
    pub fn cookies(&self) -> impl Iterator<Item = &cookie::Cookie<'static>> + '_ {
        self.cookies.iter()
    }

    async fn from(res: reqwest::Response) -> Self {
        let headers = res.headers().clone();
        let status = res.status();
        let url = res.url().clone();
        
        #[cfg(feature = "cookies")]
        let cookies: Vec<cookie::Cookie<'static>> = res.cookies()
            .map(|cookie| {
                cookie::Cookie::build((cookie.name().to_string(), cookie.value().to_string()))
                    .build()
            })
            .collect();
        
        let text = res.text().await.unwrap_or_default();
        
        Response {
            headers,
            status,
            url,
            text,
            #[cfg(feature = "cookies")]
            cookies,
        }
    }
}

/// tanu's http client that is compatible to `reqwest::Client`.
#[derive(Clone, Default)]
pub struct Client {
    pub(crate) inner: reqwest::Client,
}

impl Client {
    /// Construct tanu's HTTP client.
    pub fn new() -> Client {
        #[cfg(feature = "cookies")]
        let inner = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap_or_default();
        
        #[cfg(not(feature = "cookies"))]
        let inner = reqwest::Client::default();
        
        Client { inner }
    }

    pub fn get(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        let url = url.into_url().unwrap();
        debug!("Requesting {url}");
        RequestBuilder {
            inner: Some(self.inner.get(url)),
            client: self.inner.clone(),
        }
    }

    pub fn post(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        let url = url.into_url().unwrap();
        debug!("Requesting {url}");
        RequestBuilder {
            inner: Some(self.inner.post(url)),
            client: self.inner.clone(),
        }
    }

    pub fn put(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        let url = url.into_url().unwrap();
        debug!("Requesting {url}");
        RequestBuilder {
            inner: Some(self.inner.put(url)),
            client: self.inner.clone(),
        }
    }

    pub fn patch(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        let url = url.into_url().unwrap();
        debug!("Requesting {url}");
        RequestBuilder {
            inner: Some(self.inner.patch(url)),
            client: self.inner.clone(),
        }
    }

    pub fn delete(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        let url = url.into_url().unwrap();
        debug!("Requesting {url}");
        RequestBuilder {
            inner: Some(self.inner.delete(url)),
            client: self.inner.clone(),
        }
    }

    pub fn head(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        let url = url.into_url().unwrap();
        debug!("Requesting {url}");
        RequestBuilder {
            inner: Some(self.inner.head(url)),
            client: self.inner.clone(),
        }
    }
}

pub struct RequestBuilder {
    pub(crate) inner: Option<reqwest::RequestBuilder>,
    pub(crate) client: reqwest::Client,
}

impl RequestBuilder {
    pub fn header<K, V>(mut self, key: K, value: V) -> RequestBuilder
    where
        header::HeaderName: TryFrom<K>,
        <header::HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        header::HeaderValue: TryFrom<V>,
        <header::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.header(key, value));
        self
    }

    pub fn headers(mut self, headers: header::HeaderMap) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.headers(headers));
        self
    }

    pub fn basic_auth<U, P>(mut self, username: U, password: Option<P>) -> RequestBuilder
    where
        U: std::fmt::Display,
        P: std::fmt::Display,
    {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.basic_auth(username, password));
        self
    }

    pub fn bearer_auth<T>(mut self, token: T) -> RequestBuilder
    where
        T: std::fmt::Display,
    {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.bearer_auth(token));
        self
    }

    pub fn body<T: Into<reqwest::Body>>(mut self, body: T) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.body(body));
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.query(query));
        self
    }

    pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.form(form));
        self
    }

    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> RequestBuilder {
        self.inner = self.inner.take().map(|inner| inner.json(json));
        self
    }

    #[cfg(feature = "multipart")]
    pub fn multipart(mut self, multipart: reqwest::multipart::Form) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.multipart(multipart));
        self
    }

    pub async fn send(mut self) -> Result<Response, Error> {
        let req = self.inner.take().ok_or_eyre("inner missing")?.build()?;

        let log_request = LogRequest {
            url: req.url().clone(),
            method: req.method().clone(),
            headers: req.headers().clone(),
        };

        let time_req = Instant::now();
        let res = self.client.execute(req).await;

        match res {
            Ok(res) => {
                let res = Response::from(res).await;
                let duration_req = time_req.elapsed();

                let log_response = LogResponse {
                    headers: res.headers.clone(),
                    body: res.text.clone(),
                    status: res.status(),
                    duration_req,
                };

                crate::runner::publish(crate::runner::EventBody::Http(Box::new(Log {
                    request: log_request.clone(),
                    response: log_response,
                })))?;
                Ok(res)
            }
            Err(e) => {
                crate::runner::publish(crate::runner::EventBody::Http(Box::new(Log {
                    request: log_request,
                    response: Default::default(),
                })))?;
                Err(e.into())
            }
        }
    }

    pub fn timeout(mut self, timeout: std::time::Duration) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.timeout(timeout));
        self
    }

    pub fn try_clone(&self) -> Option<RequestBuilder> {
        let inner = self.inner.as_ref()?;
        Some(RequestBuilder {
            inner: Some(inner.try_clone()?),
            client: self.client.clone(),
        })
    }

    pub fn version(mut self, version: Version) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.version(version));
        self
    }
}
