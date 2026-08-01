use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Return whether an address is suitable for an ordinary public-web crawl.
///
/// This intentionally rejects more than Rust's `is_global`: unspecified,
/// loopback, private/ULA, link-local, multicast, IPv4 documentation ranges,
/// benchmarking space, carrier-grade NAT, and IPv4-mapped IPv6 addresses are
/// never valid public crawler destinations.
pub fn address_is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => ipv4_is_public(v4),
        IpAddr::V6(v6) => ipv6_is_public(v6),
    }
}

fn ipv4_is_public(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn ipv6_is_public(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
    {
        return false;
    }
    // Reject IPv4-compatible/mapped forms instead of maintaining two subtly
    // different policy paths for the same endpoint.
    ip.to_ipv4_mapped().is_none() && ip.to_ipv4().is_none()
}

/// Require a non-empty answer set in which every address passes public-web
/// policy. Mixed public/private DNS answers fail closed.
pub fn validate_resolved_addresses(addresses: &[SocketAddr]) -> bool {
    !addresses.is_empty() && addresses.iter().all(|a| address_is_public(a.ip()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ipv4_ssrf_and_special_ranges() {
        for ip in [
            "0.0.0.0",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
        ] {
            assert!(!address_is_public(ip.parse().unwrap()), "{ip}");
        }
        assert!(address_is_public("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn rejects_ipv6_local_documentation_and_mapped_ranges() {
        for ip in [
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!address_is_public(ip.parse().unwrap()), "{ip}");
        }
        assert!(address_is_public("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn mixed_dns_answers_fail_closed() {
        let addresses = [
            "1.1.1.1:443".parse().unwrap(),
            "127.0.0.1:443".parse().unwrap(),
        ];
        assert!(!validate_resolved_addresses(&addresses));
        assert!(!validate_resolved_addresses(&[]));
    }
}
