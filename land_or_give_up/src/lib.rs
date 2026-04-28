use serde::{Deserialize, Serialize};
use solana_sdk::clock::Slot;
use solana_sdk::transaction::Transaction;
use zela_std::{CustomProcedure, RpcClient, RpcError, Signature, zela_custom_procedure};

#[derive(Debug, Serialize, Deserialize)]
pub struct Input {
    transaction: Transaction,
    signature: Signature,
    slot: Slot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    signature: Signature,
}

pub struct ProcedureInterview {}

impl CustomProcedure for ProcedureInterview {
    type ErrorData = ();
    type Params = Input;
    type SuccessData = Output;

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Error;

    async fn run(params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        let rpc = RpcClient::new();

        let block_time = rpc.get_block_time(params.slot).await?;
        let recent_blockhas = rpc.get_latest_blockhash().await?;
        let response = rpc.send_and_confirm_transaction(&params.transaction).await;

        Err(RpcError {
            code: 404,
            message: String::from("Not found"),
            data: None,
        })
    }
}

zela_custom_procedure!(ProcedureInterview);
