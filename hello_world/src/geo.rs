//! Static IPv4 -> (coarse continent, Zela region) lookup. The CIDR table is
//! generated at build time from `data/ip_geo.tsv`; runtime is `O(log N)`
//! binary search over a sorted, disjoint range table — no allocation,
//! no I/O.

use std::net::IpAddr;

use serde::Serialize;

/// Coarse approximate location of the leader. Continent-level granularity —
/// no country codes. Add a variant here AND extend `build.rs`
/// `ALLOWED_LEADER_GEOS` in lockstep.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaderGeo {
    Europe,
    Americas,
    Africa,
    MiddleEast,
    Asia,
    Oceania,
}

/// Zela deployment regions. Add a variant here AND extend `build.rs`
/// `ALLOWED_REGIONS` in lockstep.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZelaRegion {
    Frankfurt,
    Dubai,
    NewYork,
    Tokyo,
}

#[derive(Clone, Copy, Debug)]
pub struct GeoEntry {
    pub leader_geo: LeaderGeo,
    pub region: ZelaRegion,
}

include!(concat!(env!("OUT_DIR"), "/ip_geo.rs"));

/// Returns the geo entry whose CIDR contains `ip`, or `None` if `ip` is
/// outside the table. IPv4 only; IPv4-mapped IPv6 is accepted, pure IPv6
/// returns `None`.
pub fn lookup(ip: IpAddr) -> Option<GeoEntry> {
    let v4 = match ip {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(v6) => v6.to_ipv4_mapped()?,
    };
    let n = u32::from(v4);
    let idx = IP_GEO_RANGES.partition_point(|(start, _, _)| *start <= n);
    if idx == 0 {
        return None;
    }
    let (start, end, entry) = IP_GEO_RANGES[idx - 1];
    (n >= start && n <= end).then_some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn lookup_v4(a: u8, b: u8, c: u8, d: u8) -> Option<GeoEntry> {
        lookup(IpAddr::V4(Ipv4Addr::new(a, b, c, d)))
    }

    #[test]
    fn frankfurt_hetzner_46_4() {
        // 46.4.0.0/16 -> Europe / Frankfurt
        let e = lookup_v4(46, 4, 0, 0).unwrap();
        assert_eq!(e.leader_geo, LeaderGeo::Europe);
        assert_eq!(e.region, ZelaRegion::Frankfurt);
        let e = lookup_v4(46, 4, 255, 255).unwrap();
        assert_eq!(e.region, ZelaRegion::Frankfurt);
    }

    #[test]
    fn newyork_digitalocean_24_144() {
        let e = lookup_v4(24, 144, 64, 0).unwrap();
        assert_eq!(e.leader_geo, LeaderGeo::Americas);
        assert_eq!(e.region, ZelaRegion::NewYork);
    }

    #[test]
    fn tokyo_ntt_202_8_8() {
        let e = lookup_v4(202, 8, 8, 128).unwrap();
        assert_eq!(e.leader_geo, LeaderGeo::Asia);
        assert_eq!(e.region, ZelaRegion::Tokyo);
    }

    #[test]
    fn dubai_digitalocean_bangalore() {
        let e = lookup_v4(139, 59, 0, 0).unwrap();
        assert_eq!(e.leader_geo, LeaderGeo::Asia);
        assert_eq!(e.region, ZelaRegion::Dubai);
    }

    #[test]
    fn outside_table_returns_none() {
        assert!(lookup_v4(8, 8, 8, 8).is_none());
        assert!(lookup_v4(24, 144, 63, 255).is_none());
        assert!(lookup_v4(46, 5, 0, 0).is_none());
    }

    #[test]
    fn ipv4_mapped_ipv6_resolves() {
        let mapped = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x2e04, 0));
        let e = lookup(mapped).unwrap();
        assert_eq!(e.region, ZelaRegion::Frankfurt);
    }

    #[test]
    fn pure_ipv6_returns_none() {
        let v6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        assert!(lookup(v6).is_none());
    }
}
