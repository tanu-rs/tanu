/// tanu's HTTP client is a wrapper for `reqwest::Client` and offers * exactly same interface as `reqwest::Client`
/// * to capture reqnest and response logs
use eyre::{OptionExt, WrapErr};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::{
    ops::Deref,
    sync::{Arc, Mutex},
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
}

pub struct RequestBuilder {
    pub(crate) inner: Option<reqwest::RequestBuilder>,
    pub(crate) client: reqwest::Client,
}

impl RequestBuilder {
    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> RequestBuilder {
        self.inner = self.inner.take().map(|inner| inner.json(json));
        self
    }

    pub async fn send(mut self) -> Result<Response, Error> {
        let req = self.inner.take().ok_or_eyre("inner missing")?.build()?;

        let log_request = LogRequest {
            url: req.url().clone(),
            method: req.method().clone(),
            headers: req.headers().clone(),
        };

        let res = self.client.execute(req).await;

        match res {
            Ok(res) => {
                let res = Response::from(res).await;
                let log_response = LogResponse {
                    headers: res.headers.clone(),
                    body: res.text.clone(),
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
}
