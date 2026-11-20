use std::{
    io::{Error, ErrorKind},
    net::{IpAddr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{anyhow, ensure, Context, Result};
use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    redirect::Policy,
    Client, Url,
};
use serde::de::DeserializeOwned;

static CLIENT: OnceLock<reqwest::Result<Client>> = OnceLock::new();

/// Fetch JSON from a public HTTP(S) destination with an application-specific size limit.
pub(crate) async fn fetch<T: DeserializeOwned>(uri: &str, limit: usize) -> Result<T> {
    let url = public_url(uri).context("expected a public HTTP(S) URL")?;
    let client = CLIENT
        .get_or_init(build_client)
        .as_ref()
        .map_err(|error| anyhow!("could not build the outbound HTTP client: {error}"))?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(reqwest::Error::without_url)?;
    ensure!(
        response.status().is_success(),
        "HTTP status {}",
        response.status()
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= limit as u64),
        "JSON response exceeds {limit} bytes"
    );

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(reqwest::Error::without_url)?
    {
        append_body(&mut body, &chunk, limit)?;
    }
    serde_json::from_slice(&body).context("invalid JSON response")
}

fn append_body(body: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<()> {
    ensure!(
        chunk.len() <= limit.saturating_sub(body.len()),
        "JSON response exceeds {limit} bytes"
    );
    body.extend_from_slice(chunk);
    Ok(())
}

fn build_client() -> reqwest::Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .no_proxy()
        .redirect(Policy::none())
        .dns_resolver(Arc::new(PublicDns))
        .build()
}

fn public_url(input: &str) -> Option<Url> {
    let url = Url::parse(input).ok()?;
    let host = url.host_str()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }

    // The connector skips DNS resolution for IP literals.
    if let Ok(ip) = host.trim_matches(&['[', ']'][..]).parse::<IpAddr>() {
        if !is_public(ip) {
            return None;
        }
    }
    Some(url)
}

struct PublicDns;

impl Resolve for PublicDns {
    fn resolve(&self, host: Name) -> Resolving {
        Box::pin(async move {
            // Return the checked addresses directly to the connector, so it never
            // performs a second, unchecked lookup between validation and connection.
            let addresses = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            Ok(Box::new(public_addresses(addresses)?.into_iter()) as Addrs)
        })
    }
}

fn public_addresses(addresses: Vec<SocketAddr>) -> std::io::Result<Vec<SocketAddr>> {
    if addresses.is_empty() || addresses.iter().any(|addr| !is_public(addr.ip())) {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "metadata host must resolve only to public IP addresses",
        ));
    }
    Ok(addresses)
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !matches!(
            ip.octets(),
            [0 | 10 | 127 | 224..=255, ..]
                | [100, 64..=127, ..]
                | [169, 254, ..]
                | [172, 16..=31, ..]
                | [192, 0, 0 | 2, _]
                | [192, 88, 99, _]
                | [192, 168, ..]
                | [198, 18 | 19, ..]
                | [198, 51, 100, _]
                | [203, 0, 113, _]
        ),
        IpAddr::V6(ip) => {
            // Allow global unicast, except special-purpose, transition and
            // documentation prefixes in that range.
            let [first, second, ..] = ip.segments();
            (0x2000..=0x3fff).contains(&first)
                && !matches!(
                    (first, second),
                    (0x2001, 0..=0x01ff | 0x0db8) | (0x2002, _) | (0x3fff, 0..=0x0fff)
                )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_http_endpoints_are_accepted() {
        assert!(public_url("https://example.org/metadata.json").is_some());
        assert!(public_url("ipfs://example.org/metadata.json").is_none());
        assert!(public_url("https://user@example.org/metadata.json").is_none());
        for ip in [
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ] {
            assert!(!is_public(ip));
        }
        let public = SocketAddr::from(([8, 8, 8, 8], 0));
        let non_public = SocketAddr::from(([127, 0, 0, 1], 0));
        assert!(public_addresses(vec![public]).is_ok());
        assert!(public_addresses(vec![public, non_public]).is_err());
        assert!(public_addresses(Vec::new()).is_err());
    }

    #[test]
    fn body_limit_applies_across_chunks() -> Result<()> {
        let mut bytes = Vec::new();
        append_body(&mut bytes, b"{", 2)?;
        append_body(&mut bytes, b"}", 2)?;
        assert!(append_body(&mut bytes, b" ", 2).is_err());
        assert_eq!(bytes, b"{}");
        Ok(())
    }
}
