use serde::{Deserialize, Serialize};
use zela_std::solana_sdk::clock::Slot;

#[cfg(not(target_arch = "wasm32"))]
use solana_client::nonblocking::rpc_client::RpcClient;
#[cfg(target_arch = "wasm32")]
use zela_std::RpcClient;

mod geo;

use geo::{LeaderGeo, ZelaRegion};

pub struct HelloWorld;

#[derive(Serialize, Deserialize)]
pub struct Input {
    pub empty: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub slot: Slot,
    pub leader: String,
    /// Coarse, continent-level approximation of the leader's location.
    /// `None` when the leader's IP is not in the static geo table.
    pub leader_geo: Option<LeaderGeo>,
    /// Closest Zela region to the current leader. `None` when the leader's IP
    /// is not in the static geo table (treat as "unknown" client-side).
    pub closest_region: Option<ZelaRegion>,
}

impl HelloWorld {
    pub async fn run(_: Input, rpc: &RpcClient) -> Result<Output, String> {
        let epoch_info = rpc.get_epoch_info().await.map_err(|e| e.to_string())?;
        let current_slot = epoch_info.absolute_slot;
        let slot_index = epoch_info.slot_index as usize;

        let leader = rpc
            .get_leader_schedule(Some(current_slot))
            .await
            .map_err(|e| e.to_string())?
            .and_then(|schedule| {
                schedule
                    .into_iter()
                    .find(|(_, offsets)| offsets.contains(&slot_index))
                    .map(|(pubkey, _)| pubkey)
            })
            .ok_or_else(|| format!("no leader scheduled for slot_index={slot_index}"))?;

        let leader_ip = rpc
            .get_cluster_nodes()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|n| n.pubkey == leader)
            .and_then(|n| n.gossip.or(n.tpu).or(n.rpc))
            .map(|addr| addr.ip());

        let geo_entry = leader_ip.and_then(geo::lookup);
        let leader_geo = geo_entry.map(|e| e.leader_geo);
        let closest_region = geo_entry.map(|e| e.region);

        log::debug!(
            "slot={current_slot} slot_index={slot_index} leader={leader} \
             ip={leader_ip:?} leader_geo={leader_geo:?} region={closest_region:?}"
        );

        Ok(Output {
            slot: current_slot,
            leader,
            leader_geo,
            closest_region,
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod zela {
    use super::*;
    use zela_std::{CustomProcedure, RpcError, zela_custom_procedure};

    impl CustomProcedure for HelloWorld {
        type Params = Input;
        type ErrorData = ();
        type SuccessData = Output;

        async fn run(params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
            let rpc = RpcClient::new();
            match Self::run(params, &rpc).await {
                Ok(v) => Ok(v),
                Err(message) => Err(RpcError {
                    code: 1,
                    message,
                    data: None,
                }),
            }
        }

        const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
    }

    zela_custom_procedure!(HelloWorld);
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::net::IpAddr;

    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_commitment_config::CommitmentConfig;

    fn init_logger() {
        let _ = env_logger::builder()
            .is_test(true)
            .parse_env(env_logger::Env::new().default_filter_or("info,hello_world=debug"))
            .try_init();
    }

    fn mainnet_rpc() -> RpcClient {
        RpcClient::new_with_commitment(
            "https://api.mainnet-beta.solana.com".to_string(),
            CommitmentConfig::confirmed(),
        )
    }

    #[tokio::test]
    async fn test_procedure_local() {
        init_logger();
        let rpc = mainnet_rpc();

        let out = HelloWorld::run(
            Input {
                empty: String::new(),
            },
            &rpc,
        )
        .await
        .unwrap();
        log::warn!("Test output: {out:?}");
    }

    /// Scans every cluster node, tallies geo coverage, and prints the top
    /// unmatched /16 prefixes so the operator knows what to add to
    /// `data/ip_geo.tsv` next.
    #[tokio::test]
    async fn test_cluster_geo_coverage() {
        init_logger();
        let rpc = mainnet_rpc();

        let nodes = rpc.get_cluster_nodes().await.unwrap();
        let total = nodes.len();

        let mut by_region: HashMap<ZelaRegion, u32> = HashMap::new();
        let mut unmatched: HashMap<String, u32> = HashMap::new();
        let mut ipv6_only = 0u32;
        let mut no_ip = 0u32;

        for n in &nodes {
            let Some(addr) = n.gossip.or(n.tpu).or(n.rpc) else {
                no_ip += 1;
                continue;
            };
            let ip = addr.ip();
            match geo::lookup(ip) {
                Some(entry) => *by_region.entry(entry.region).or_default() += 1,
                None => match ip {
                    IpAddr::V4(v4) => {
                        let o = v4.octets();
                        let key = format!("{}.{}.0.0/16", o[0], o[1]);
                        *unmatched.entry(key).or_default() += 1;
                    }
                    IpAddr::V6(_) => ipv6_only += 1,
                },
            }
        }

        let matched: u32 = by_region.values().sum();
        log::warn!(
            "geo coverage: {matched}/{total} matched ({:.1}%), ipv6-only={ipv6_only}, no-ip={no_ip}",
            100.0 * matched as f64 / total as f64
        );
        for region in [
            ZelaRegion::Frankfurt,
            ZelaRegion::Dubai,
            ZelaRegion::NewYork,
            ZelaRegion::Tokyo,
        ] {
            log::warn!(
                "  {region:?}: {}",
                by_region.get(&region).copied().unwrap_or(0)
            );
        }

        let mut top: Vec<_> = unmatched.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        log::warn!("Top 20 unmatched /16 prefixes (count cidr):");
        for (cidr, count) in top.iter().take(20) {
            log::warn!("  {count:>4}  {cidr}");
        }
    }
}
