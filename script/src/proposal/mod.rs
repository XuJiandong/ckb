pub mod count_vote;
#[cfg(feature = "probe")]
pub mod probe;
#[cfg(test)]
mod tests;

use crate::proposal::count_vote::count_vote;
use crate::{ScriptError, ScriptGroup};
use ckb_hash::new_blake2b;
#[cfg(feature = "skip-checking")]
use ckb_logger::warn;
use ckb_types::{
    core::{Cycle, cell::ResolvedTransaction},
    packed::Proposal,
    prelude::*,
};

pub use ckb_traits::{BlockProvider, CellDataProvider};

/// Combined provider trait for types that serve both block and cell data.
/// Needed because Rust disallows `dyn BlockProvider + CellDataProvider` directly.
pub trait BlockAndCellProvider: BlockProvider + CellDataProvider {}

impl<T: BlockProvider + CellDataProvider + ?Sized> BlockAndCellProvider for T {}

#[cfg(feature = "probe")]
struct VerifyGuard;

#[cfg(feature = "probe")]
impl VerifyGuard {
    fn new() -> Self {
        probe::proposal_probe::verify_entry!(|| ());
        VerifyGuard
    }
}

#[cfg(feature = "probe")]
impl Drop for VerifyGuard {
    fn drop(&mut self) {
        probe::proposal_probe::verify_exit!(|| ());
    }
}

// TODO:
pub const PROPOSAL_CYCLES: Cycle = 50_000_000;

/// Minimum capacity (in shannon) that a proposal cell must lock at creation
/// time as an anti-spam deposit. Set to 1000 CKBytes.
///
/// If a proposal fails (or is never settled), the cell is unspendable, so this
/// capacity is permanently lost — the cost of spamming.
pub const MIN_PROPOSAL_CAPACITY: u64 = 100_000_000_000;

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
pub const ERROR_OVERFLOW: i8 = -13;
pub const ERROR_UNEXPECTED: i8 = -14;
pub const ERROR_INSUFFICIENT_CAPACITY: i8 = -15;

pub struct ProposalTypeSystemScript<'a, B: CellDataProvider + BlockProvider + ?Sized> {
    pub rtx: &'a ResolvedTransaction,
    pub script_group: &'a ScriptGroup,
    pub max_cycles: Cycle,
    pub block_provider: &'a B,
}

impl<'a, B: CellDataProvider + BlockProvider + ?Sized> ProposalTypeSystemScript<'a, B> {
    pub fn verify(&self) -> Result<Cycle, ScriptError> {
        #[cfg(feature = "probe")]
        let _verify_guard = VerifyGuard::new();

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
        let mut blake160 = [0; 20];
        blake160.copy_from_slice(&ret[..20]);

        if blake160[..] != self.script_group.script.args().raw_data()[..] {
            return Err(self.validation_failure(ERROR_INVALID_INPUT_HASH));
        }
        let output = self
            .rtx
            .transaction
            .data()
            .raw()
            .outputs()
            .get(first_output_index as usize)
            .ok_or_else(|| self.validation_failure(ERROR_ARGS))?;
        let capacity: u64 = output.capacity().unpack();
        if capacity < MIN_PROPOSAL_CAPACITY {
            return Err(self.validation_failure(ERROR_INSUFFICIENT_CAPACITY));
        }

        // TODO: verify the proposal cell data is valid
        //    - `vote_cell_code_hash` / `vote_cell_hash_type`
        //    - `duration`
        //    - `amount`
        //    - `minimal_requirement`

        // default cycles for creating
        Ok(1_000_000)
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
            #[cfg(feature = "skip-checking")]
            {
                warn!("Skip checking. for benchmark only.")
            }
            #[cfg(not(feature = "skip-checking"))]
            {
                return Err(self.validation_failure(ERROR_INVALID_START_BLOCK));
            }
        }

        // Resolved inputs are not eager-loaded (`mem_cell_data` is usually None).
        // Load the proposal cell data directly from storage.
        let proposal = {
            let cell_data = match resolved_input.mem_cell_data.as_ref() {
                Some(data) => data.clone(),
                None => self
                    .block_provider
                    .get_cell_data(&resolved_input.out_point)
                    .ok_or_else(|| self.validation_failure(ERROR_PARSE_CELL_DATA))?,
            };
            Proposal::from_compatible_slice(&cell_data)
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
        // Rough bounds: reject zero / absurdly large durations that would stall verification.
        // Concrete product ranges still TBD (see creation-time validation TODO).
        if !(1..=60_000).contains(&duration) {
            return Err(self.validation_failure(ERROR_PARSE_CELL_DATA));
        }
        #[cfg(feature = "probe")]
        probe::proposal_probe::block_provider_entry!(|| ());
        let start_header_opt = self.block_provider.get_block_header(&header_dep_0);
        #[cfg(feature = "probe")]
        probe::proposal_probe::block_provider_exit!(|| ());
        let start_header =
            start_header_opt.ok_or_else(|| self.validation_failure(ERROR_INVALID_START_BLOCK))?;
        #[cfg(feature = "probe")]
        probe::proposal_probe::block_provider_entry!(|| ());
        let end_header_opt = self.block_provider.get_block_header(&header_dep_1);
        #[cfg(feature = "probe")]
        probe::proposal_probe::block_provider_exit!(|| ());
        let end_header =
            end_header_opt.ok_or_else(|| self.validation_failure(ERROR_BLOCK_RANGE_INVALID))?;

        let expected_end_number = start_header.number() + duration as u64;
        if end_header.number() != expected_end_number {
            return Err(self.validation_failure(ERROR_BLOCK_RANGE_INVALID));
        }

        let (result, cycles) = count_vote(
            self.block_provider,
            &proposal,
            &self.script_group.script,
            start_header.number(),
        )?;

        if result.passed {
            Ok(cycles)
        } else {
            #[cfg(feature = "skip-checking")]
            {
                warn!("Skip checking. for benchmark only.");
                Ok(cycles)
            }
            #[cfg(not(feature = "skip-checking"))]
            {
                Err(self.validation_failure(ERROR_PROPOSAL_FAILED))
            }
        }
    }

    fn validation_failure(&self, exit_code: i8) -> ScriptError {
        ScriptError::validation_failure(&self.script_group.script, exit_code)
    }
}
