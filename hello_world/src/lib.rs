use serde::{Deserialize, Serialize};
use zela_std::solana_sdk::clock::Slot;
use zela_std::{CustomProcedure, RpcClient, RpcError, zela_custom_procedure};

mod geo;

use geo::ZelaRegion;

pub struct HelloWorld;

#[derive(Serialize, Deserialize)]
pub struct Input {
    pub empty: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub slot: Slot,
    pub leader: String,
    /// Closest Zela region to the current leader. `None` when the leader's IP
    /// is not in the static geo table (treat as "unknown" client-side).
    pub closest_region: Option<ZelaRegion>,
}

impl CustomProcedure for HelloWorld {
    type Params = Input;
    type ErrorData = ();
    type SuccessData = Output;

    async fn run(_: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        let rpc = RpcClient::new();

        let epoch_info = rpc.get_epoch_info().await?;
        let current_slot = epoch_info.absolute_slot;
        let slot_index = epoch_info.slot_index as usize;

        let leader = rpc
            .get_leader_schedule(Some(current_slot))
            .await?
            .and_then(|schedule| {
                schedule
                    .into_iter()
                    .find(|(_, offsets)| offsets.contains(&slot_index))
                    .map(|(pubkey, _)| pubkey)
            })
            .ok_or_else(|| RpcError {
                code: 404,
                message: format!("no leader scheduled for slot_index={slot_index}"),
                data: None,
            })?;

        let leader_ip = rpc
            .get_cluster_nodes()
            .await?
            .into_iter()
            .find(|n| n.pubkey == leader)
            .and_then(|n| n.gossip.or(n.tpu).or(n.rpc))
            .map(|addr| addr.ip());

        let closest_region = leader_ip.and_then(geo::lookup);

        log::debug!(
            "slot={current_slot} slot_index={slot_index} leader={leader} \
             ip={leader_ip:?} region={closest_region:?}"
        );

        Ok(Output {
            slot: current_slot,
            leader,
            closest_region,
        })
    }

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
}

zela_custom_procedure!(HelloWorld);
