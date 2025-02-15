//! Engine API types: <https://github.com/ethereum/execution-apis/blob/main/src/engine/authentication.md> and <https://eips.ethereum.org/EIPS/eip-3675> following the execution spec <https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md>

#![allow(missing_docs)]

use reth_primitives::{Address, BlockNumber, Bloom, Bytes, H256, H64, U256, U64};
use serde::{Deserialize, Serialize};

/// This structure maps on the ExecutionPayload structure of the beacon chain spec.
///
/// See also: <https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#executionpayloadv1>
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayload {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Bloom,
    pub prev_randao: H256,
    pub block_number: U64,
    pub gas_limit: U64,
    pub gas_used: U64,
    pub timestamp: U64,
    pub extra_data: Bytes,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions: Vec<Bytes>,
    /// Array of [`Withdrawal`] enabled with V2
    /// See <https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#executionpayloadv2>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withdrawal: Option<Withdrawal>,
}

/// This structure maps onto the validator withdrawal object from the beacon chain spec.
///
/// See also: <https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#withdrawalv1>
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Withdrawal {
    pub index: U64,
    pub validator_index: U64,
    pub address: Address,
    /// Note: the amount value is represented on the beacon chain as a little-endian value in units
    /// of Gwei, whereas the amount in this structure MUST be converted to a big-endian value in
    /// units of Wei.
    pub amount: U256,
}

/// This structure encapsulates the fork choice state
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceState {
    pub head_block_hash: H256,
    pub safe_block_hash: H256,
    pub finalized_block_hash: H256,
}

/// This structure contains the attributes required to initiate a payload build process in the
/// context of an `engine_forkchoiceUpdated` call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadAttributes {
    pub timestamp: U64,
    pub prev_randao: H256,
    pub suggested_fee_recipient: Address,
    /// Array of [`Withdrawal`] enabled with V2
    /// See <https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#executionpayloadv2>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withdrawal: Option<Withdrawal>,
}

/// This structure contains the result of processing a payload
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadStatus {
    #[serde(flatten)]
    pub status: PayloadStatusEnum,
    /// Hash of the most recent valid block in the branch defined by payload and its ancestors
    pub latest_valid_hash: Option<H256>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PayloadStatusEnum {
    Valid,
    Invalid {
        #[serde(rename = "validationError")]
        validation_error: String,
    },
    Syncing,
    Accepted,
    InvalidBlockHash {
        #[serde(rename = "validationError")]
        validation_error: String,
    },
}

/// This structure contains configurable settings of the transition process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionConfiguration {
    /// Maps on the TERMINAL_TOTAL_DIFFICULTY parameter of EIP-3675
    pub terminal_total_difficulty: U256,
    /// Maps on TERMINAL_BLOCK_HASH parameter of EIP-3675
    pub terminal_block_hash: H256,
    /// Maps on TERMINAL_BLOCK_NUMBER parameter of EIP-3675
    pub terminal_block_number: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceUpdated {
    pub payload_status: PayloadStatus,
    pub payload_id: Option<H64>,
}
