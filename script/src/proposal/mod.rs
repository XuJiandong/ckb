pub mod count_vote;
#[cfg(test)]
mod tests;
mod types;

pub use types::*;

use crate::proposal::count_vote::count_vote;
use crate::{ScriptError, ScriptGroup};
use ckb_hash::new_blake2b;
use ckb_types::{
    core::{BlockView, Cycle, HeaderView, cell::ResolvedTransaction},
    packed::Byte32,
    prelude::*,
};

pub const PROPOSAL_CYCLES: Cycle = 10_000_000;

pub const ERROR_ARGS: i8 = -1;
pub const ERROR_TOO_MANY_CELLS: i8 = -2;
pub const ERROR_INVALID_INPUT_HASH: i8 = -3;
pub const ERROR_REQUIRED_FIELD: i8 = -4;
pub const ERROR_PROPOSAL_BOTH_SIDES: i8 = -5;
pub const ERROR_PROPOSAL_NOT_FOUND: i8 = -6;
pub const ERROR_BLOCK_COUNT_MISMATCH: i8 = -7;
pub const ERROR_BLOCK_RANGE_INVALID: i8 = -8;
pub const ERROR_PROPOSAL_FAILED: i8 = -9;
pub const ERROR_INVALID_START_BLOCK: i8 = -10;
pub const ERROR_MISSING_HEADER_DEPS: i8 = -11;
pub const ERROR_PARSE_CELL_DATA: i8 = -12;

pub trait BlockProvider {
    fn get_block(&self, hash: &Byte32) -> Option<BlockView>;
    fn get_block_header(&self, hash: &Byte32) -> Option<HeaderView>;
    fn get_block_by_number(&self, number: u64) -> Option<BlockView>;
}

pub struct ProposalTypeSystemScript<'a, B: BlockProvider + ?Sized> {
    pub rtx: &'a ResolvedTransaction,
    pub script_group: &'a ScriptGroup,
    pub max_cycles: Cycle,
    pub block_provider: &'a B,
}

impl<'a, B: BlockProvider + ?Sized> ProposalTypeSystemScript<'a, B> {
    pub fn verify(&self) -> Result<Cycle, ScriptError> {
        if self.max_cycles < PROPOSAL_CYCLES {
            return Err(ScriptError::ExceededMaximumCycles(self.max_cycles));
        }

        if !self.script_group.input_indices.is_empty()
            && !self.script_group.output_indices.is_empty()
        {
            return Err(self.validation_failure(ERROR_PROPOSAL_BOTH_SIDES));
        }

        if self.script_group.input_indices.is_empty() {
            self.verify_creation()
        } else {
            self.verify_consumption()
        }
    }

    fn verify_creation(&self) -> Result<Cycle, ScriptError> {
        if self.script_group.output_indices.len() > 1 {
            return Err(self.validation_failure(ERROR_TOO_MANY_CELLS));
        }

        let first_cell_input = self
            .rtx
            .transaction
            .data()
            .raw()
            .inputs()
            .get(0)
            .ok_or_else(|| self.validation_failure(ERROR_ARGS))?;
        let first_output_index: u64 = self
            .script_group
            .output_indices
            .first()
            .map(|output_index| *output_index as u64)
            .ok_or_else(|| self.validation_failure(ERROR_ARGS))?;

        let mut blake2b = new_blake2b();
        blake2b.update(first_cell_input.as_slice());
        blake2b.update(&first_output_index.to_le_bytes());
        let mut ret = [0; 32];
        blake2b.finalize(&mut ret);

        if ret[..] != self.script_group.script.args().raw_data()[..] {
            return Err(self.validation_failure(ERROR_INVALID_INPUT_HASH));
        }

        Ok(PROPOSAL_CYCLES)
    }

    fn verify_consumption(&self) -> Result<Cycle, ScriptError> {
        if self.script_group.input_indices.len() > 1 {
            return Err(self.validation_failure(ERROR_TOO_MANY_CELLS));
        }

        let input_index = self.script_group.input_indices[0];
        let resolved_input = self
            .rtx
            .resolved_inputs
            .get(input_index)
            .ok_or_else(|| self.validation_failure(ERROR_ARGS))?;

        let tx_info = resolved_input
            .transaction_info
            .as_ref()
            .ok_or_else(|| self.validation_failure(ERROR_ARGS))?;
        let start_block_hash = tx_info.block_hash.clone();

        let header_deps = self.rtx.transaction.data().raw().header_deps();
        let header_dep_0 = header_deps
            .get(0)
            .ok_or_else(|| self.validation_failure(ERROR_MISSING_HEADER_DEPS))?;
        let header_dep_1 = header_deps
            .get(1)
            .ok_or_else(|| self.validation_failure(ERROR_MISSING_HEADER_DEPS))?;

        if header_dep_0.as_slice() != start_block_hash.as_slice() {
            return Err(self.validation_failure(ERROR_INVALID_START_BLOCK));
        }

        let proposal = {
            let cell_data = resolved_input
                .mem_cell_data
                .as_ref()
                .ok_or_else(|| self.validation_failure(ERROR_PARSE_CELL_DATA))?;
            Proposal::from_compatible_slice(cell_data)
                .map_err(|_| self.validation_failure(ERROR_PARSE_CELL_DATA))?
        };

        let proposal_reader = proposal.as_reader();
        let duration = u32::from_le_bytes(
            proposal_reader
                .duration()
                .as_slice()
                .try_into()
                .map_err(|_| self.validation_failure(ERROR_PARSE_CELL_DATA))?,
        );

        let start_header = self
            .block_provider
            .get_block_header(&header_dep_0)
            .ok_or_else(|| self.validation_failure(ERROR_INVALID_START_BLOCK))?;
        let end_header = self
            .block_provider
            .get_block_header(&header_dep_1)
            .ok_or_else(|| self.validation_failure(ERROR_BLOCK_RANGE_INVALID))?;

        let expected_end_number = start_header.number() + duration as u64;
        if end_header.number() != expected_end_number {
            return Err(self.validation_failure(ERROR_BLOCK_RANGE_INVALID));
        }

        let mut blocks: Vec<BlockView> = Vec::with_capacity(duration as usize + 1);
        blocks.push(
            self.block_provider
                .get_block(&header_dep_0)
                .ok_or_else(|| self.validation_failure(ERROR_INVALID_START_BLOCK))?,
        );
        for block_number in (start_header.number() + 1)..=end_header.number() {
            let block = self
                .block_provider
                .get_block_by_number(block_number)
                .ok_or_else(|| self.validation_failure(ERROR_BLOCK_COUNT_MISMATCH))?;
            blocks.push(block);
        }

        let result = count_vote(&blocks, &self.script_group.script);

        if result.passed {
            Ok(PROPOSAL_CYCLES)
        } else {
            Err(self.validation_failure(ERROR_PROPOSAL_FAILED))
        }
    }

    fn validation_failure(&self, exit_code: i8) -> ScriptError {
        ScriptError::validation_failure(&self.script_group.script, exit_code)
    }
}
