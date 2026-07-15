use ckb_types::{
    core::{BlockNumber, BlockView, HeaderView},
    packed::Byte32,
};

/// Trait for loading full blocks, used by embedded scripts that need block body access.
pub trait BlockProvider {
    /// Get a full block by header hash.
    fn get_block(&self, hash: &Byte32) -> Option<BlockView>;
    /// Get a block header by header hash.
    fn get_block_header(&self, hash: &Byte32) -> Option<HeaderView>;
    /// Get a full block by block number.
    fn get_block_by_number(&self, number: BlockNumber) -> Option<BlockView>;
}
