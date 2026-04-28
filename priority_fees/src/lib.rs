use serde::{Deserialize, Serialize};

use solana_transaction_status_client_types::{EncodedTransaction, TransactionDetails, UiMessage, UiTransactionEncoding};
#[cfg(target_arch = "wasm32")]
use zela_std::rpc_client::{RpcClient, RpcBlockConfig};
#[cfg(not(target_arch = "wasm32"))]
use solana_client::{
	rpc_config::RpcBlockConfig,
	nonblocking::rpc_client::RpcClient
};

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum Input {
	Latest {
		block_count: usize
	},
	Specific {
		blocks: Vec<u64>
	}
}

#[derive(Serialize, Debug)]
pub struct Output {
	/// Total number of transactions scanned.
	total_transactions: usize,
	/// Number of transaction skipped because they are voting.
	vote_transactions: usize,
	/// Latest processed block.
	latest_block: u64,
	// Average priority fees paid per non-voting transactions
	average_priority_fee_lamports: u64
}

pub struct PriorityFees;
impl PriorityFees {
	/// Divisor for computing a margin when fetching blocks
	const BLOCK_COUNT_SLOT_MARGIN_DIV: usize = 10;
	/// Base fee every transactions pays.
	const BASE_FEE: u64 = 5000;
	const VOTE_ACCOUNT: &'static str = "Vote111111111111111111111111111111111111111";

	/// Number of times we'll widen the lookback window before giving up.
	const MAX_LOOKBACK_ATTEMPTS: usize = 8;

	/// Selects blocks according to input and returns their slot numbers.
	async fn select_blocks(p: Input, rpc: &RpcClient) -> Result<Vec<u64>, String> {
		let block_count = match p {
			Input::Specific { blocks } => return Ok(blocks),
			Input::Latest { block_count } => block_count,
		};
		if block_count == 0 {
			return Ok(Vec::new());
		}

		// start off with some latest slot number - it doesn't need to be the absolute latest,
		// just needs to be close (at our commitment level)
		let latest_slot = rpc.get_slot().await.map_err(|e| e.to_string())?;

		// Estimated lookback window with a 10% margin for skipped slots.
		let window = (block_count + block_count / Self::BLOCK_COUNT_SLOT_MARGIN_DIV + 1) as u64;
		let mut start_slot = latest_slot.saturating_sub(window);

		// Find slot numbers of latest block_count blocks. The previous version
		// looped on the SAME start_slot when too few finalized blocks came back,
		// retrying the same RPC indefinitely. Now we widen the window backwards
		// each attempt and bail after MAX_LOOKBACK_ATTEMPTS so a degraded chain
		// cannot wedge the procedure.
		let mut block_slots = Vec::<u64>::new();
		for attempt in 0..Self::MAX_LOOKBACK_ATTEMPTS {
			block_slots = rpc.get_blocks_with_commitment(
				start_slot,
				None,
				rpc.commitment()
			).await.map_err(|e| e.to_string())?;
			log::trace!(
				"get_blocks({start_slot}..) attempt={attempt} got={}",
				block_slots.len()
			);
			if block_slots.len() >= block_count {
				break;
			}
			if start_slot == 0 {
				return Err(format!(
					"only {} block(s) available back to genesis, need {}",
					block_slots.len(),
					block_count
				));
			}
			// Widen the lookback by another full window.
			start_slot = start_slot.saturating_sub(window);
		}
		if block_slots.len() < block_count {
			return Err(format!(
				"could not collect {} block(s) within {} attempts (got {})",
				block_count,
				Self::MAX_LOOKBACK_ATTEMPTS,
				block_slots.len()
			));
		}
		log::info!("Got {} latest blocks: {:?}", block_slots.len(), block_slots);

		let to_skip = block_slots.len() - block_count;

		Ok(block_slots.split_off(to_skip))
	}

	pub async fn run(p: Input, rpc: &RpcClient) -> Result<Output, String> {
		log::debug!("run({p:?})");

		let mut total_fees: u64 = 0;
		let mut nonvote_count: usize = 0;
		let mut total_count: usize = 0;
		let mut latest_block: u64 = 0;

		for slot in Self::select_blocks(p, rpc).await? {
			log::debug!("Processing block {slot}");
			let block = rpc.get_block_with_config(
				slot,
				RpcBlockConfig {
					encoding: Some(UiTransactionEncoding::Json),
					transaction_details: Some(TransactionDetails::Full),
					rewards: None,
					commitment: Some(rpc.commitment()),
					max_supported_transaction_version: Some(0),
				}
			).await.map_err(|e| e.to_string())?;
			let transactions = match block.transactions {
				Some(t) => t,
				None => {
					log::error!("Transactions not found (block={})", slot);
					continue;
				}
			};
			total_count += transactions.len();
			latest_block = slot;

			for (i, transaction) in transactions.into_iter().enumerate() {
				log::trace!("transaction: {transaction:#?}");

				let is_voting = match transaction.transaction {
					EncodedTransaction::Json(t) => match t.message {
						UiMessage::Parsed(m) => m.account_keys.iter().any(|k| k.pubkey == Self::VOTE_ACCOUNT),
						UiMessage::Raw(m) => m.account_keys.iter().any(|k| k == Self::VOTE_ACCOUNT)
					}
					_ => {
						log::error!("Transaction account keys not found (block={}, idx={})", slot, i);
						continue;
					}
				};
				// skip voting transactions
				if is_voting {
					continue;
				}

				let priority_fee = match transaction.meta {
					Some(m) if m.fee < Self::BASE_FEE => {
						log::error!("Transaction fee less than base fee (block={}, idx={})", slot, i);
						continue;
					}
					Some(m) => m.fee - Self::BASE_FEE,
					None => {
						log::error!("Transaction fee not found (block={}, idx={})", slot, i);
						continue;
					}
				};

				total_fees += priority_fee;
				nonvote_count += 1;
			}
		}

		// Guard against div-by-zero: nonvote_count == 0 happens for
		// `Input::Latest { block_count: 0 }`, blocks containing only vote txs,
		// or blocks where every tx was skipped due to missing meta. Returning 0
		// is the pragmatic choice — the procedure response contract has no
		// "no data" variant, and total_transactions/vote_transactions already
		// communicate that the sample was empty of qualifying txs.
		let average_priority_fee_lamports = if nonvote_count == 0 {
			0
		} else {
			total_fees / (nonvote_count as u64)
		};

		Ok(Output {
			total_transactions: total_count,
			vote_transactions: total_count - nonvote_count,
			latest_block,
			average_priority_fee_lamports,
		})
	}
}

#[cfg(target_arch = "wasm32")]
mod zela {
	use zela_std::{zela_custom_procedure, CustomProcedure, RpcError};

	use super::*;

	impl CustomProcedure for PriorityFees {
		type Params = Input;
		type ErrorData = ();
		type SuccessData = Output;

		// Run method is the entry point of every custom procedure
		// It will be called once for each incoming request.
		async fn run(params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
			let rpc = RpcClient::new();

			match Self::run(params, &rpc).await {
				Ok(v) => Ok(v),
				Err(err) => Err(RpcError {
					code: 1,
					message: err,
					data: None
				})
			}
		}

		const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
	}
	zela_custom_procedure!(PriorityFees);
}

#[cfg(test)]
mod tests {
	use super::*;

	use solana_client::nonblocking::rpc_client::RpcClient;
	use solana_commitment_config::CommitmentConfig;

	#[tokio::test]
	async fn test_procedure_local() {
		env_logger::builder()
			.is_test(true)
			.parse_env(env_logger::Env::new().default_filter_or("info,priority_fees=debug"))
			.init();

		let rpc = RpcClient::new_with_commitment(
			"https://api.mainnet-beta.solana.com".to_string(),
			CommitmentConfig::confirmed(),
		);

		let out = PriorityFees::run(Input::Latest {
			block_count: 1
		}, &rpc).await.unwrap();
		log::warn!("Test output: {out:?}");
	}
}
