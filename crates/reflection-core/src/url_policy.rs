use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use url::Url;

use crate::{Result, RkError};

pub fn parse_and_validate_url(input: &str) -> Result<Url> {
    let url = parse_url_with_default_scheme(input)?;
    validate_url(&url)?;
    Ok(url)
}

fn parse_url_with_default_scheme(input: &str) -> Result<Url> {
    let trimmed = input.trim();
    match Url::parse(trimmed) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) if looks_like_bare_host_url(trimmed) => {
            Ok(Url::parse(&format!("https://{trimmed}"))?)
        }
        Err(error) => Err(error.into()),
    }
}

fn looks_like_bare_host_url(value: &str) -> bool {
    let Some(host_port) = value.split(['/', '?', '#']).next() else {
        return false;
    };
    if host_port.is_empty()
        || host_port.contains('@')
        || host_port.contains(char::is_whitespace)
        || value.starts_with("//")
    {
        return false;
    }
    let host = host_port
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_else(|| host_port.split(':').next().unwrap_or(host_port));
    host.eq_ignore_ascii_case("localhost") || host.contains('.')
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
        || a == 0
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
    // Fold any IPv6 form that embeds or tunnels an IPv4 address back to that
    // IPv4 address and apply the v4 rules. Without this, IPv4-mapped addresses
    // such as ::ffff:169.254.169.254 (cloud metadata) or ::ffff:127.0.0.1
    // slip past the v6 checks because is_loopback()/is_private() are false for
    // them. Covers IPv4-mapped (::ffff:0:0/96), 6to4 (2002::/16),
    // NAT64/well-known prefix (64:ff9b::/96), and Teredo (2001::/32).
    if let Some(v4) = embedded_ipv4(ip) {
        return is_blocked_ipv4(v4);
    }

    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || is_ipv6_unique_local(ip)
        || is_ipv6_unicast_link_local(ip)
        || is_ipv6_documentation(ip)
}

/// Extract an embedded/tunneled IPv4 address from an IPv6 address, if any.
fn embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let seg = ip.segments();

    // IPv4-mapped (::ffff:a.b.c.d). Deliberately NOT to_ipv4(): that also
    // matches IPv4-compatible ::/96, folding ::1 to 0.0.0.1 and slipping past
    // the v4 rules. is_loopback()/is_unspecified() already cover ::1 and ::.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return Some(v4);
    }

    // 6to4: 2002:AABB:CCDD::/48 embeds a.b.c.d in segments[1..=2].
    if seg[0] == 0x2002 {
        let [a, b] = seg[1].to_be_bytes();
        let [c, d] = seg[2].to_be_bytes();
        return Some(Ipv4Addr::new(a, b, c, d));
    }

    // NAT64 well-known prefix 64:ff9b::/96 embeds a.b.c.d in the low 32 bits.
    if seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 && seg[5] == 0
    {
        let [a, b] = seg[6].to_be_bytes();
        let [c, d] = seg[7].to_be_bytes();
        return Some(Ipv4Addr::new(a, b, c, d));
    }

    // Teredo 2001:0000::/32 embeds the (obfuscated) server/client IPv4. Treat
    // the embedded client address (last two segments, bitwise negated) as the
    // target to validate.
    if seg[0] == 0x2001 && seg[1] == 0x0000 {
        let [a, b] = (!seg[6]).to_be_bytes();
        let [c, d] = (!seg[7]).to_be_bytes();
        return Some(Ipv4Addr::new(a, b, c, d));
    }

    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_ipv6_embedded_ipv4_ssrf_vectors() {
        // (address, expected_blocked) — mapped/6to4/NAT64/Teredo forms that must
        // fold back to IPv4 and hit the v4 rules.
        let cases: &[(&str, bool)] = &[
            // IPv4-mapped loopback / metadata / private / CGNAT / link-local
            ("::ffff:127.0.0.1", true),
            ("::ffff:169.254.169.254", true),
            ("::ffff:10.0.0.1", true),
            ("::ffff:192.168.1.1", true),
            ("::ffff:172.16.0.1", true),
            ("::ffff:100.64.0.1", true),
            ("::ffff:0.0.0.0", true),
            // IPv4-mapped public address must stay allowed
            ("::ffff:1.1.1.1", false),
            ("::ffff:8.8.8.8", false),
            // 6to4 wrapping metadata / private
            ("2002:a9fe:a9fe::", true),   // 169.254.169.254
            ("2002:0a00:0001::", true),   // 10.0.0.1
            ("2002:0808:0808::", false),  // 8.8.8.8 public
            // NAT64 well-known prefix wrapping metadata / private
            ("64:ff9b::a9fe:a9fe", true), // 169.254.169.254
            ("64:ff9b::0a00:0001", true), // 10.0.0.1
            ("64:ff9b::0808:0808", false),// 8.8.8.8 public
            // Plain v6 loopback / unspecified / ULA / link-local stay blocked
            ("::1", true),
            ("::", true),
            ("fc00::1", true),
            ("fe80::1", true),
            ("2001:db8::1", true),
            // Public v6 allowed
            ("2606:4700:4700::1111", false),
        ];
        for (addr, expected) in cases {
            let ip: std::net::Ipv6Addr = addr.parse().unwrap();
            assert_eq!(
                is_blocked_ipv6(ip),
                *expected,
                "is_blocked_ipv6({addr}) expected {expected}"
            );
        }
    }

    #[test]
    fn bare_host_urls_default_to_https() {
        let parsed = parse_and_validate_url("www.youtube.com/watch").unwrap();
        assert_eq!(parsed.as_str(), "https://www.youtube.com/watch");

        let parsed = parse_and_validate_url("example.com/watch?v=1").unwrap();
        assert_eq!(parsed.as_str(), "https://example.com/watch?v=1");
    }

    #[test]
    fn explicit_http_urls_are_preserved() {
        let parsed = parse_and_validate_url("http://example.com/watch").unwrap();
        assert_eq!(parsed.as_str(), "http://example.com/watch");
    }

    #[test]
    fn non_url_text_is_not_treated_as_host() {
        assert!(parse_and_validate_url("not a url").is_err());
        assert!(parse_and_validate_url("/watch?v=1").is_err());
    }
}
