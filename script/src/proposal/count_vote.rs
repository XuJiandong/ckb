#![allow(missing_docs)]
use super::{BlockProvider, ERROR_BLOCK_COUNT_MISMATCH, PROPOSAL_CYCLES, Proposal, Vote};
use crate::ScriptError;
use ckb_hash::blake2b_256;
use ckb_types::{
    core::{BlockView, Cycle},
    packed::Script,
    prelude::*,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct VoteResult {
    pub proposal: Proposal,
    pub yes_vote: u64,
    pub no_vote: u64,
    pub passed: bool,
}

fn blake160(data: &[u8]) -> [u8; 20] {
    let hash = blake2b_256(data);
    let mut result = [0u8; 20];
    result.copy_from_slice(&hash[..20]);
    result
}

fn process_block(
    block: &BlockView,
    vote_code_hash: &[u8],
    vote_hash_type: &[u8],
    proposal_blake160: &[u8; 20],
    vote_map: &mut BTreeMap<[u8; 32], (u8, u64)>,
    dao_outpoint_to_voter: &mut BTreeMap<[u8; 36], [u8; 32]>,
    cycles: &mut Cycle,
) -> Result<(), ScriptError> {
    *cycles += (block.data().total_size() as Cycle) * 4;
    if *cycles > PROPOSAL_CYCLES {
        return Err(ScriptError::ExceededMaximumCycles(*cycles));
    }

    for tx in block.transactions() {
        let data = tx.data();
        let raw = data.raw();

        for input in raw.inputs().into_iter() {
            let op_bytes: [u8; 36] = input
                .previous_output()
                .as_slice()
                .try_into()
                .expect("OutPoint is always 36 bytes");
            if let Some(voter_lock_hash) = dao_outpoint_to_voter.remove(&op_bytes) {
                vote_map.remove(&voter_lock_hash);
                *cycles += 1000;
                if *cycles > PROPOSAL_CYCLES {
                    return Err(ScriptError::ExceededMaximumCycles(*cycles));
                }
            }
        }

        let outputs = raw.outputs();
        let outputs_data = raw.outputs_data();
        let cell_deps = raw.cell_deps();

        for j in 0..outputs.len() {
            let output = outputs.get(j).expect("should exist");
            let type_script = match output.type_().to_opt() {
                Some(t) => t,
                None => continue,
            };

            if type_script.code_hash().as_slice() != vote_code_hash {
                continue;
            }
            if type_script.hash_type().as_slice() != vote_hash_type {
                continue;
            }
            if type_script.args().raw_data() != proposal_blake160[..] {
                continue;
            }

            let cell_data = match outputs_data.get(j) {
                Some(d) => d,
                None => continue,
            };

            let vote = match Vote::from_compatible_slice(&cell_data.raw_data()) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let voter_lock_hash = blake2b_256(output.lock().as_slice());
            let direction = vote.as_reader().vote().as_slice()[0];
            let amount = u64::from_le_bytes(
                vote.as_reader()
                    .amount()
                    .as_slice()
                    .try_into()
                    .expect("Uint64 is 8 bytes"),
            );

            for idx_reader in vote.as_reader().dao_index().iter() {
                let idx = u16::from_le_bytes(idx_reader.as_slice().try_into().unwrap()) as usize;
                if let Some(cell_dep) = cell_deps.get(idx) {
                    let op_bytes: [u8; 36] = cell_dep
                        .out_point()
                        .as_slice()
                        .try_into()
                        .expect("OutPoint is always 36 bytes");
                    dao_outpoint_to_voter.insert(op_bytes, voter_lock_hash);
                    *cycles += 1000;
                    if *cycles > PROPOSAL_CYCLES {
                        return Err(ScriptError::ExceededMaximumCycles(*cycles));
                    }
                }
            }
            vote_map.insert(voter_lock_hash, (direction, amount));
            *cycles += 1000;
            if *cycles > PROPOSAL_CYCLES {
                return Err(ScriptError::ExceededMaximumCycles(*cycles));
            }
        }
    }

    Ok(())
}

pub fn count_vote<B: BlockProvider + ?Sized>(
    block_provider: &B,
    proposal: &Proposal,
    proposal_script: &Script,
    start_number: u64,
) -> Result<(VoteResult, Cycle), ScriptError> {
    let mut cycles: Cycle = 1_000_000;

    let proposal_reader = proposal.as_reader();
    let vote_code_hash = proposal_reader.vote_cell_code_hash();
    let vote_hash_type = proposal_reader.vote_cell_hash_type();
    let minimal_req = u64::from_le_bytes(
        proposal_reader
            .minimal_requirement()
            .as_slice()
            .try_into()
            .expect("Uint64 is 8 bytes"),
    );
    let duration = u32::from_le_bytes(
        proposal_reader
            .duration()
            .as_slice()
            .try_into()
            .expect("Uint32 is 4 bytes"),
    ) as u64;

    let proposal_blake160: [u8; 20] = blake160(proposal_script.as_slice());

    let mut vote_map: BTreeMap<[u8; 32], (u8, u64)> = BTreeMap::new();
    let mut dao_outpoint_to_voter: BTreeMap<[u8; 36], [u8; 32]> = BTreeMap::new();

    for block_number in (start_number + 1)..=(start_number + duration) {
        let block = block_provider
            .get_block_by_number(block_number)
            .ok_or_else(|| {
                ScriptError::validation_failure(proposal_script, ERROR_BLOCK_COUNT_MISMATCH)
            })?;
        process_block(
            &block,
            vote_code_hash.as_slice(),
            vote_hash_type.as_slice(),
            &proposal_blake160,
            &mut vote_map,
            &mut dao_outpoint_to_voter,
            &mut cycles,
        )?;
    }

    let mut yes_vote: u64 = 0;
    let mut no_vote: u64 = 0;
    for (direction, amount) in vote_map.values() {
        if *direction == 1 {
            yes_vote = yes_vote.saturating_add(*amount);
        } else {
            no_vote = no_vote.saturating_add(*amount);
        }
    }

    let total_vote = yes_vote.saturating_add(no_vote);
    let min_shannon = minimal_req * 100_000_000;
    let passed = yes_vote > no_vote && total_vote > min_shannon;

    Ok((
        VoteResult {
            proposal: proposal.clone(),
            yes_vote,
            no_vote,
            passed,
        },
        cycles,
    ))
}
