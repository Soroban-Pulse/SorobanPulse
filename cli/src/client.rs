use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

use crate::config::Config;

pub struct ApiClient {
    inner:    Client,
    base_url: String,
    api_key:  String,
}

impl ApiClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let inner = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .context("building HTTP client")?;

        Ok(Self {
            inner,
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            api_key:  cfg.api_key.clone(),
        })
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str, params: &[(&str, &str)]) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .inner
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(params)
            .send()
            .with_context(|| format!("GET {url}"))?;

        self.parse(resp, &url)
    }

    pub fn get_raw(&self, path: &str, params: &[(&str, &str)]) -> Result<Response> {
        let url = format!("{}{}", self.base_url, path);
        self.inner
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(params)
            .send()
            .with_context(|| format!("GET {url}"))
    }

    /// POST a JSON body and deserialize the response.
    pub fn post<B: serde::Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .inner
            .post(&url)
            .header("x-api-key", &self.api_key)
            .json(body)
            .send()
            .with_context(|| format!("POST {url}"))?;

        self.parse(resp, &url)
    }

    /// POST with no request body, deserializing the response.
    pub fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .inner
            .post(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .with_context(|| format!("POST {url}"))?;

        self.parse(resp, &url)
    }

    /// Send a DELETE request. Returns `Ok(())` on any 2xx response — most
    /// delete endpoints reply with an empty body or a small status object
    /// we don't need to parse.
    pub fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .inner
            .delete(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .with_context(|| format!("DELETE {url}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("HTTP {status}: {body}");
        }
        Ok(())
    }

    fn parse<T: DeserializeOwned>(&self, resp: Response, url: &str) -> Result<T> {
        let status = resp.status();
        let body = resp.text().with_context(|| format!("reading body from {url}"))?;

        if !status.is_success() {
            // Try to extract a message from the error body
            let msg = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").or_else(|| v.get("message")).cloned())
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| body.clone());
            anyhow::bail!("HTTP {status}: {msg}");
        }

        serde_json::from_str(&body)
            .with_context(|| format!("parsing JSON response from {url}"))
    }
}
