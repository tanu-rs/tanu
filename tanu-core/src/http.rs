/// tanu's HTTP client is a wrapper for `reqwest::Client` and offers * exactly same interface as `reqwest::Client`
/// * to capture reqnest and response logs
use eyre::{OptionExt, WrapErr};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::{
    ops::Deref,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;
use tracing::*;

tokio::task_local! {
    pub static CHANNEL: Arc<Mutex<Option<broadcast::Sender<Log>>>>;
}

/// Subscribe to the channel to see the real-time network logs.
pub fn subscribe() -> eyre::Result<broadcast::Receiver<Log>> {
    let ch = CHANNEL.get();
    let Ok(guard) = ch.lock() else {
        eyre::bail!("failed to acquire http channel lock");
    };
    let Some(tx) = guard.deref() else {
        eyre::bail!("http channel has been already closed");
    };

    Ok(tx.subscribe())
}

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
    pub url: reqwest::Url,
    pub method: reqwest::Method,
    pub headers: reqwest::header::HeaderMap,
}

#[derive(Debug, Clone, Default)]
pub struct LogResponse {
    pub headers: reqwest::header::HeaderMap,
    pub body: String,
    pub status: reqwest::StatusCode,
    pub duration_req: Duration,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub request: LogRequest,
    pub response: LogResponse,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub headers: reqwest::header::HeaderMap,
    pub status: reqwest::StatusCode,
    pub text: String,
}

impl Response {
    pub fn status(&self) -> reqwest::StatusCode {
        self.status
    }

    pub fn headers(&self) -> &reqwest::header::HeaderMap {
        &self.headers
    }

    pub async fn text(self) -> Result<String, Error> {
        Ok(self.text)
    }

    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, Error> {
        Ok(serde_json::from_str(&self.text)?)
    }

    async fn from(res: reqwest::Response) -> Self {
        Response {
            headers: res.headers().clone(),
            status: res.status(),
            text: res.text().await.unwrap_or_default(),
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
        Client::default()
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
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.header(key, value));
        self
    }

    pub fn headers(mut self, headers: HeaderMap) -> RequestBuilder {
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

                let ch = CHANNEL.get();
                let Ok(guard) = ch.lock() else {
                    return Err(eyre::eyre!("failed to acquire http channel lock").into());
                };
                if let Some(tx) = guard.deref() {
                    tx.send(Log {
                        request: log_request,
                        response: log_response,
                    })
                    .wrap_err("failed to send a message to http channel")?;
                }
                Ok(res)
            }
            Err(e) => {
                let ch = CHANNEL.get();
                let Ok(guard) = ch.lock() else {
                    return Err(eyre::eyre!("failed to acquire http channel lock").into());
                };
                if let Some(tx) = guard.deref() {
                    tx.send(Log {
                        request: log_request,
                        response: LogResponse::default(),
                    })
                    .wrap_err("failed to send a message to http channel")?;
                }
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

    pub fn version(mut self, version: reqwest::Version) -> RequestBuilder {
        let inner = self.inner.take().expect("inner missing");
        self.inner = Some(inner.version(version));
        self
    }
}
