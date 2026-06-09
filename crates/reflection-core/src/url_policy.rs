use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use url::Url;

use crate::{Result, RkError};

pub fn parse_and_validate_url(input: &str) -> Result<Url> {
    let url = Url::parse(input)?;
    validate_url(&url)?;
    Ok(url)
}

pub fn validate_url(url: &Url) -> Result<()> {
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(RkError::UrlPolicy(format!("unsupported scheme `{scheme}`"))),
    }

    let host = url
        .host_str()
        .ok_or_else(|| RkError::UrlPolicy("missing host".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(443);

    for socket_addr in (host, port)
        .to_socket_addrs()
        .map_err(|error| RkError::UrlPolicy(format!("DNS resolution failed: {error}")))?
    {
        if is_blocked_ip(socket_addr.ip()) {
            return Err(RkError::UrlPolicy(format!(
                "host resolves to blocked address {}",
                socket_addr.ip()
            )));
        }
    }

    Ok(())
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_blocked_ipv4(ip),
        IpAddr::V6(ip) => is_blocked_ipv6(ip),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _d] = ip.octets();

    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || is_ipv6_unique_local(ip)
        || is_ipv6_unicast_link_local(ip)
        || is_ipv6_documentation(ip)
}

fn is_ipv6_unique_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_ipv6_unicast_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn is_ipv6_documentation(ip: Ipv6Addr) -> bool {
    ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8
}
