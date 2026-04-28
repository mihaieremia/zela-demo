use serde::Serialize;
use zela_std::solana_sdk::clock::Slot;
use zela_std::{CustomProcedure, RpcClient, RpcError, zela_custom_procedure};

// mod geo;

pub struct HelloWorld;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub slot: Slot,
    pub leader: String,
    pub leader_geo: String,
    pub leader_asn: Option<u32>,
    pub closest_region: String,
}

const UNKNOWN: &str = "unknown";

impl CustomProcedure for HelloWorld {
    type Params = ();
    type ErrorData = ();
    type SuccessData = Output;

    async fn run(_: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        let rpc = RpcClient::new();

        let epoch_info = rpc.get_epoch_info().await?;
        let current_slot = epoch_info.absolute_slot;
        let slot_index = epoch_info.slot_index as usize;

        let leader = match rpc.get_leader_schedule(Some(current_slot)).await? {
            Some(schedule) => schedule
                .into_iter()
                .find(|(_, offsets)| offsets.contains(&slot_index))
                .map(|(pubkey, _)| pubkey),
            None => None,
        };

        let leader = match leader {
            Some(pk) => pk,
            None => {
                log::warn!("no leader found for slot_index={slot_index}");
                return Ok(Output {
                    slot: current_slot,
                    leader: UNKNOWN.to_string(),
                    leader_geo: UNKNOWN.to_string(),
                    leader_asn: None,
                    closest_region: UNKNOWN.to_string(),
                });
            }
        };

        let leader_ip = rpc
            .get_cluster_nodes()
            .await?
            .into_iter()
            .find(|n| n.pubkey == leader)
            .and_then(|n| n.gossip.or(n.tpu).or(n.rpc))
            .map(|addr| addr.ip());

        log::debug!("leader_ip={leader_ip:?}");

        // let geo_entry = leader_ip.and_then(geo::lookup);
        // let leader_geo = geo_entry.map(|g| g.country).unwrap_or(UNKNOWN);
        // let closest_region = geo::country_to_region(leader_geo);

        // log::debug!(
        //     "slot={current_slot} slot_index={slot_index} leader={leader} ip={leader_ip:?} \
        //      country={leader_geo} region={closest_region}"
        // );

        // Ok(Output {
        //     slot: current_slot,
        //     leader,
        //     leader_geo: "leader_geo".to_string(),
        //     leader_asn: geo_entry.map(|g| g.asn),
        //     closest_region: closest_region.to_string(),
        // })
        //
        return Ok(Output {
            slot: current_slot,
            leader: UNKNOWN.to_string(),
            leader_geo: UNKNOWN.to_string(),
            leader_asn: None,
            closest_region: UNKNOWN.to_string(),
        });
    }

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
}

zela_custom_procedure!(HelloWorld);
