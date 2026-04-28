//! Static IPv4 -> Zela region lookup. The CIDR table is generated at build
//! time from `data/ip_geo.tsv`; runtime is `O(log N)` binary search over a
//! sorted, disjoint range table — no allocation, no I/O.

use std::net::IpAddr;

use serde::Serialize;

/// Zela deployment regions. Add a variant here AND extend `build.rs`
/// `ALLOWED_REGIONS` in lockstep.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZelaRegion {
    Frankfurt,
    Dubai,
    NewYork,
    Tokyo,
}

include!(concat!(env!("OUT_DIR"), "/ip_geo.rs"));

/// Returns the Zela region whose CIDR contains `ip`, or `None` if `ip` is
/// outside the table. IPv4 only; IPv4-mapped IPv6 addresses are accepted,
/// pure IPv6 returns `None`.
pub fn lookup(ip: IpAddr) -> Option<ZelaRegion> {
    let v4 = match ip {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(v6) => v6.to_ipv4_mapped()?,
    };
    let n = u32::from(v4);
    let idx = IP_GEO_RANGES.partition_point(|(start, _, _)| *start <= n);
    if idx == 0 {
        return None;
    }
    let (start, end, region) = IP_GEO_RANGES[idx - 1];
    (n >= start && n <= end).then_some(region)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn lookup_v4(a: u8, b: u8, c: u8, d: u8) -> Option<ZelaRegion> {
        lookup(IpAddr::V4(Ipv4Addr::new(a, b, c, d)))
    }

    #[test]
    fn frankfurt_hetzner_46_4() {
        // 46.4.0.0/16 -> Frankfurt
        assert_eq!(lookup_v4(46, 4, 0, 0), Some(ZelaRegion::Frankfurt));
        assert_eq!(lookup_v4(46, 4, 255, 255), Some(ZelaRegion::Frankfurt));
    }

    #[test]
    fn newyork_digitalocean_24_144() {
        // 24.144.64.0/19 -> NewYork
        assert_eq!(lookup_v4(24, 144, 64, 0), Some(ZelaRegion::NewYork));
        assert_eq!(lookup_v4(24, 144, 95, 255), Some(ZelaRegion::NewYork));
    }

    #[test]
    fn tokyo_ntt_202_8_8() {
        // 202.8.8.0/24 -> Tokyo (single /24)
        assert_eq!(lookup_v4(202, 8, 8, 0), Some(ZelaRegion::Tokyo));
        assert_eq!(lookup_v4(202, 8, 8, 255), Some(ZelaRegion::Tokyo));
    }

    #[test]
    fn dubai_digitalocean_bangalore() {
        // 139.59.0.0/16 -> Dubai (S. Asia routes to Dubai)
        assert_eq!(lookup_v4(139, 59, 0, 0), Some(ZelaRegion::Dubai));
    }

    #[test]
    fn outside_table_returns_none() {
        // 8.8.8.8 (Google DNS) — not in seed table.
        assert_eq!(lookup_v4(8, 8, 8, 8), None);
        // One below the first range start.
        assert_eq!(lookup_v4(24, 144, 63, 255), None);
        // One above 46.4.0.0/16.
        assert_eq!(lookup_v4(46, 5, 0, 0), None);
    }

    #[test]
    fn ipv4_mapped_ipv6_resolves() {
        // ::ffff:46.4.0.0 -> Frankfurt via to_ipv4_mapped.
        let mapped = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x2e04, 0));
        assert_eq!(lookup(mapped), Some(ZelaRegion::Frankfurt));
    }

    #[test]
    fn pure_ipv6_returns_none() {
        let v6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        assert_eq!(lookup(v6), None);
    }
}
