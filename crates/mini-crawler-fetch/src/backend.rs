use std::{future::Future, net::SocketAddr, pin::Pin, time::Duration};

use reqwest::{header, redirect::Policy, Client};

/// Transport failures are deliberately coarse in public observations; the
/// detailed string is local diagnostics and must not be presented as a stable
/// protocol value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    Timeout,
    Resolve,
    Connect(String),
    ResponseTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Injectable DNS/HTTP boundary. Implementations must connect only to the
/// supplied, already-approved addresses; resolving the host again would reopen
/// DNS-rebinding and redirect-escape attacks.
pub trait FetchBackend: std::fmt::Debug + Send + Sync {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
        port: u16,
    ) -> BackendFuture<'a, Result<Vec<SocketAddr>, BackendError>>;

    fn get<'a>(
        &'a self,
        url: &'a str,
        host: &'a str,
        approved_addresses: &'a [SocketAddr],
        max_body_bytes: usize,
        request_timeout: Duration,
        connect_timeout: Duration,
    ) -> BackendFuture<'a, Result<RawResponse, BackendError>>;
}

#[derive(Debug, Clone)]
pub struct ReqwestBackend {
    user_agent: String,
}

impl ReqwestBackend {
    pub fn new(user_agent: impl Into<String>) -> Result<Self, BackendError> {
        let user_agent = user_agent.into();
        if user_agent.trim().is_empty() || user_agent.contains(['\r', '\n']) {
            return Err(BackendError::Connect("invalid user agent".into()));
        }
        Ok(Self { user_agent })
    }
}

impl FetchBackend for ReqwestBackend {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
        port: u16,
    ) -> BackendFuture<'a, Result<Vec<SocketAddr>, BackendError>> {
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| BackendError::Resolve)?
                .collect::<Vec<_>>();
            Ok(addresses)
        })
    }

    fn get<'a>(
        &'a self,
        url: &'a str,
        host: &'a str,
        approved_addresses: &'a [SocketAddr],
        max_body_bytes: usize,
        request_timeout: Duration,
        connect_timeout: Duration,
    ) -> BackendFuture<'a, Result<RawResponse, BackendError>> {
        Box::pin(async move {
            let client = Client::builder()
                .redirect(Policy::none())
                .connect_timeout(connect_timeout)
                .timeout(request_timeout)
                .resolve_to_addrs(host, approved_addresses)
                .build()
                .map_err(|e| BackendError::Connect(e.to_string()))?;
            let mut response = client
                .get(url)
                .header(header::USER_AGENT, &self.user_agent)
                .header(header::ACCEPT_ENCODING, "identity")
                .send()
                .await
                .map_err(classify_reqwest_error)?;

            if response
                .content_length()
                .is_some_and(|n| n > max_body_bytes as u64)
            {
                return Err(BackendError::ResponseTooLarge);
            }
            let status = response.status().as_u16();
            let content_type = header_string(response.headers(), header::CONTENT_TYPE);
            let location = header_string(response.headers(), header::LOCATION);
            let mut body = Vec::with_capacity(
                response
                    .content_length()
                    .unwrap_or(0)
                    .min(max_body_bytes as u64) as usize,
            );
            while let Some(chunk) = response.chunk().await.map_err(classify_reqwest_error)? {
                let next = body
                    .len()
                    .checked_add(chunk.len())
                    .ok_or(BackendError::ResponseTooLarge)?;
                if next > max_body_bytes {
                    return Err(BackendError::ResponseTooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(RawResponse {
                status,
                content_type,
                location,
                body,
            })
        })
    }
}

fn header_string(headers: &header::HeaderMap, name: header::HeaderName) -> Option<String> {
    headers.get(name)?.to_str().ok().map(ToOwned::to_owned)
}

fn classify_reqwest_error(error: reqwest::Error) -> BackendError {
    if error.is_timeout() {
        BackendError::Timeout
    } else {
        BackendError::Connect(error.to_string())
    }
}
