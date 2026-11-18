//! Thin GitHub HTTP client. Every response goes through `classify`, which
//! turns status codes and rate-limit headers into `Error` variants. Base
//! URLs are configurable so tests run against a local mock server.

use std::sync::Mutex;

use reqwest::header::{self, HeaderMap};
use reqwest::{Method, StatusCode};
use serde_json::Value;

use crate::{Error, Result, now};

/// Below this many remaining requests, sync pauses until the reset.
pub const LOW_REMAINING: i64 = 50;

pub struct Github {
    http: reqwest::Client,
    /// REST base, e.g. `https://api.github.com`. GraphQL is `{api}/graphql`.
    pub api: String,
    /// Web base for OAuth, e.g. `https://github.com`.
    pub web: String,
    /// Last seen `(remaining, reset)` of the core / graphql budget.
    rate: Mutex<Option<(i64, i64)>>,
}

pub struct Resp {
    pub status: u16,
    pub headers: HeaderMap,
    /// `Null` for empty bodies (204, 205, 304).
    pub body: Value,
}

impl Resp {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// URL of the next page from the `Link` header.
    pub fn next_link(&self) -> Option<String> {
        self.header("link")?.split(',').find_map(|part| {
            let (url, rel) = part.split_once(';')?;
            rel.contains("rel=\"next\"")
                .then(|| url.trim().trim_start_matches('<').trim_end_matches('>').to_owned())
        })
    }
}

impl Github {
    pub fn new(api: &str, web: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Forkline/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client with static config");
        Self {
            http,
            api: api.trim_end_matches('/').to_owned(),
            web: web.trim_end_matches('/').to_owned(),
            rate: Mutex::new(None),
        }
    }

    /// `(remaining, reset)` from the last response that carried them.
    pub fn rate(&self) -> Option<(i64, i64)> {
        *self.rate.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// REST call. `path_or_url` is either `/path` (joined to `api`) or a full
    /// URL (from a `Link` header). `extra` adds conditional headers.
    pub async fn rest(
        &self,
        token: &str,
        method: Method,
        path_or_url: &str,
        body: Option<&Value>,
        extra: &[(&str, &str)],
    ) -> Result<Resp> {
        let url = if path_or_url.starts_with("http") {
            path_or_url.to_owned()
        } else {
            format!("{}{path_or_url}", self.api)
        };
        let mut req = self
            .http
            .request(method, url)
            .bearer_auth(token)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(net)?;
        self.classify(resp).await
    }

    /// GraphQL call. Returns `data`; GraphQL `errors` become `Error::Api`
    /// (422) or `RateLimited`.
    pub async fn graphql(&self, token: &str, query: &str, vars: Value) -> Result<Value> {
        let body = serde_json::json!({ "query": query, "variables": vars });
        let url = format!("{}/graphql", self.api);
        let mut r = self.rest(token, Method::POST, &url, Some(&body), &[]).await?;
        if let Some(errors) = r.body.get("errors").and_then(Value::as_array) {
            if errors.iter().any(|e| e["type"] == "RATE_LIMITED") {
                return Err(Error::RateLimited { until: self.rate().map_or(now() + 60, |r| r.1) });
            }
            let message = errors
                .iter()
                .filter_map(|e| e["message"].as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(Error::Api { status: 422, message });
        }
        Ok(r.body["data"].take())
    }

    /// OAuth endpoints on the web host (form body, JSON answer, no token).
    pub async fn oauth(&self, path: &str, form: &[(&str, &str)]) -> Result<Value> {
        let resp = self
            .http
            .post(format!("{}{path}", self.web))
            .header(header::ACCEPT, "application/json")
            .form(form)
            .send()
            .await
            .map_err(net)?;
        Ok(self.classify(resp).await?.body)
    }

    async fn classify(&self, resp: reqwest::Response) -> Result<Resp> {
        let status = resp.status();
        let headers = resp.headers().clone();
        let num = |name: &str| {
            headers.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<i64>().ok())
        };
        let remaining = num("x-ratelimit-remaining");
        let reset = num("x-ratelimit-reset");
        if let (Some(rem), Some(reset)) = (remaining, reset) {
            *self.rate.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((rem, reset));
        }
        let retry_after = num("retry-after");
        let text = resp.text().await.map_err(net)?;
        let body: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        let code = status.as_u16();
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            return Ok(Resp { status: code, headers, body });
        }
        let message = body["message"].as_str().unwrap_or("request failed").to_owned();
        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::Unauthorized);
        }
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            if let Some(secs) = retry_after {
                return Err(Error::RateLimited { until: now() + secs });
            }
            if remaining == Some(0) {
                return Err(Error::RateLimited { until: reset.unwrap_or(now() + 60) });
            }
            if message.to_ascii_lowercase().contains("rate limit") {
                return Err(Error::RateLimited { until: now() + 60 });
            }
        }
        Err(Error::Api { status: code, message })
    }
}

/// Network error text without the URL (it may carry query parameters).
fn net(e: reqwest::Error) -> Error {
    Error::Network(e.without_url().to_string())
}
