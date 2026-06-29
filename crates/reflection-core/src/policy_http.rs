use std::{sync::Arc, time::Duration};

use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    Client, ClientBuilder, Response,
};
use tokio::net::lookup_host;

use crate::{
    url_policy::{is_blocked_ip, validate_url},
    Result, RkError,
};

#[derive(Debug, Clone, Default)]
struct PolicyDnsResolver;

impl Resolve for PolicyDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = lookup_host((host.as_str(), 0)).await.map_err(|error| {
                Box::new(std::io::Error::new(
                    error.kind(),
                    format!("DNS resolution failed for {host}: {error}"),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;
            let allowed = addrs
                .filter(|addr| !is_blocked_ip(addr.ip()))
                .collect::<Vec<_>>();
            if allowed.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("host {host} resolves only to blocked addresses"),
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(allowed.into_iter()) as Addrs)
        })
    }
}

pub fn policy_client_builder(timeout: Duration) -> ClientBuilder {
    Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .dns_resolver(Arc::new(PolicyDnsResolver))
}

pub fn policy_client(timeout: Duration, user_agent: &str) -> Result<Client> {
    Ok(policy_client_builder(timeout)
        .user_agent(user_agent)
        .build()?)
}

pub fn validate_response_peer(response: &Response) -> Result<()> {
    match response.remote_addr() {
        Some(addr) if is_blocked_ip(addr.ip()) => Err(RkError::UrlPolicy(format!(
            "remote peer resolved to blocked address {}",
            addr.ip()
        ))),
        Some(_) => Ok(()),
        None => Err(RkError::UrlPolicy(
            "remote peer address is unavailable for policy validation".to_string(),
        )),
    }
}

pub fn validate_response_url_and_peer(response: &Response) -> Result<()> {
    validate_url(response.url())?;
    validate_response_peer(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn policy_resolver_rejects_localhost_only_hosts() {
        let resolver = PolicyDnsResolver;
        let result = resolver
            .resolve("localhost".parse().expect("valid dns name"))
            .await;

        assert!(result.is_err());
    }
}
