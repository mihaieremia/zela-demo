use serde::{Deserialize, Serialize};
use zela_std::{CustomProcedure, RpcError, zela_custom_procedure};
use zela_std::rpc_client::{PubsubClient, StreamExt};

/// Hard cap so a runaway caller cannot wedge an executor slot for more than
/// ~20 s (Solana slots fire every ~400 ms).
const MAX_COUNT: usize = 50;

#[derive(Deserialize, Debug)]
pub struct Input {
	count: usize,
}

#[derive(Serialize, Debug)]
pub struct SlotEntry {
	slot: u64,
	parent: u64,
	root: u64,
}

#[derive(Serialize, Debug)]
pub struct Output {
	collected: usize,
	slots: Vec<SlotEntry>,
}

pub struct Subscribe;

impl CustomProcedure for Subscribe {
	type Params = Input;
	type ErrorData = ();
	type SuccessData = Output;

	async fn run(params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
		log::debug!("subscribe params: {params:?}");

		if params.count == 0 || params.count > MAX_COUNT {
			return Err(RpcError {
				code: 400,
				message: format!("count must be in 1..={MAX_COUNT}, got {}", params.count),
				data: None,
			});
		}

		// Open the Solana websocket subscription via the host's `rpc-subscription`
		// resource. Drop at end of `run` closes it host-side.
		let pubsub = PubsubClient::new();
		let mut stream = pubsub
			.slot_subscribe()
			.await
			.map_err(|err| RpcError {
				code: 1,
				message: format!("slot_subscribe failed: {err}"),
				data: None,
			})?;

		let mut slots = Vec::with_capacity(params.count);
		while slots.len() < params.count {
			match stream.next().await {
				Some(Ok(info)) => {
					log::trace!("slot {} parent {} root {}", info.slot, info.parent, info.root);
					slots.push(SlotEntry {
						slot: info.slot,
						parent: info.parent,
						root: info.root,
					});
				}
				Some(Err(err)) => {
					// Mid-stream error: stop and return what we have. If we got
					// nothing at all, surface as an RpcError so the caller knows
					// the subscription itself was broken.
					log::warn!("subscription stream error: {err}");
					if slots.is_empty() {
						return Err(RpcError {
							code: 1,
							message: format!("stream error before any data: {err}"),
							data: None,
						});
					}
					break;
				}
				None => {
					// Host closed the subscription. Same handling as the error
					// case — partial success or empty-stream error.
					log::warn!("subscription closed by host after {} item(s)", slots.len());
					if slots.is_empty() {
						return Err(RpcError {
							code: 1,
							message: "subscription closed before any data".into(),
							data: None,
						});
					}
					break;
				}
			}
		}

		log::info!("collected {} slot update(s)", slots.len());
		Ok(Output {
			collected: slots.len(),
			slots,
		})
	}

	const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
}

zela_custom_procedure!(Subscribe);
