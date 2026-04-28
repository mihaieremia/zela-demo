//! Static IPv4 -> (country, ASN) lookup, generated at build time from
//! `data/ip_geo.tsv`. Runtime is `O(log N)` binary search over a sorted
//! disjoint range table — no allocation, no I/O.

use std::net::{IpAddr, Ipv4Addr};

#[derive(Clone, Copy)]
pub struct IpGeo {
    pub country: &'static str,
    pub asn: u32,
}

include!(concat!(env!("OUT_DIR"), "/ip_geo.rs"));

/// Returns the geo entry whose CIDR contains `ip`, or `None` if `ip` is
/// outside the table or is IPv6 (table is IPv4-only).
pub fn lookup(ip: IpAddr) -> Option<&'static IpGeo> {
    let v4 = match ip {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(v6) => v6.to_ipv4_mapped()?,
    };
    lookup_ipv4(v4)
}

fn lookup_ipv4(ip: Ipv4Addr) -> Option<&'static IpGeo> {
    let n = u32::from(ip);
    let idx = IP_GEO_RANGES.partition_point(|(start, _, _)| *start <= n);
    if idx == 0 {
        return None;
    }
    let (start, end, geo) = &IP_GEO_RANGES[idx - 1];
    (n >= *start && n <= *end).then_some(geo)
}

/// Maps an ISO 3166-1 alpha-2 country code to the closest service region label.
/// Hand-curated; extend as the deployment footprint grows.
pub fn country_to_region(country: &str) -> &'static str {
    match country {
        "US" | "CA" | "MX" => "us-east",
        "BR" | "AR" | "CL" => "sa-east",
        "GB" | "IE" | "FR" | "NL" | "BE" | "PT" | "ES" => "eu-west",
        "DE" | "CH" | "AT" | "PL" | "CZ" | "IT" | "FI" | "SE" | "NO" | "DK" => "eu-central",
        "JP" | "KR" => "ap-northeast",
        "SG" | "HK" | "TW" | "TH" | "MY" | "ID" | "VN" | "PH" => "ap-southeast",
        "IN" => "ap-south",
        "AU" | "NZ" => "ap-southeast-2",
        "AE" | "IL" | "SA" => "me-central",
        "ZA" | "NG" | "KE" => "af-south",
        _ => "unknown",
    }
}
