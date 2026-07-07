use super::BlockProvider;
use super::count_vote::count_vote;
use super::{Proposal, Uint16Vec, Vote};
use ckb_hash::blake2b_256;
use ckb_types::{
    core::{BlockBuilder, BlockView, HeaderView, TransactionBuilder},
    packed,
    prelude::*,
};
use std::collections::BTreeMap;

#[allow(dead_code)]
struct MockBlockProvider {
    blocks: BTreeMap<u64, BlockView>,
}

#[allow(dead_code)]
impl MockBlockProvider {
    fn new(blocks: Vec<BlockView>) -> Self {
        let map: BTreeMap<u64, BlockView> = blocks.into_iter().map(|b| (b.number(), b)).collect();
        Self { blocks: map }
    }
}

impl BlockProvider for MockBlockProvider {
    fn get_block(&self, hash: &packed::Byte32) -> Option<BlockView> {
        self.blocks.values().find(|b| &b.hash() == hash).cloned()
    }

    fn get_block_header(&self, hash: &packed::Byte32) -> Option<HeaderView> {
        self.blocks
            .values()
            .find(|b| &b.hash() == hash)
            .map(|b| b.header())
    }

    fn get_block_by_number(&self, number: u64) -> Option<BlockView> {
        self.blocks.get(&number).cloned()
    }
}

fn uint32(v: u32) -> packed::Uint32 {
    packed::Uint32::from(v.to_le_bytes())
}

fn uint64(v: u64) -> packed::Uint64 {
    packed::Uint64::from(v.to_le_bytes())
}

fn epoch() -> packed::Uint64 {
    let e = ckb_types::core::EpochNumberWithFraction::new(0, 0, 1800);
    packed::Uint64::from(e)
}

fn make_proposal_script() -> packed::Script {
    let code_hash = blake2b_256(b"proposal_type_script");
    packed::Script::new_builder()
        .code_hash(packed::Byte32::from(code_hash))
        .hash_type(1u8)
        .args(packed::Bytes::default())
        .build()
}

fn make_vote_type_script(
    vote_code_hash: &[u8; 32],
    proposal_blake160: &[u8; 20],
) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed::Byte32::from(*vote_code_hash))
        .hash_type(0u8)
        .args(packed::Bytes::from(proposal_blake160.to_vec()))
        .build()
}

fn make_proposal_tx(proposal: &Proposal, proposal_script: &packed::Script) -> TransactionBuilder {
    let lock = packed::Script::default();
    let output = packed::CellOutput::new_builder()
        .lock(lock)
        .type_(
            packed::ScriptOpt::new_builder()
                .set(Some(proposal_script.clone()))
                .build(),
        )
        .build();
    TransactionBuilder::default()
        .version(uint32(0))
        .output(output)
        .output_data(packed::Bytes::from(proposal.as_slice().to_vec()))
}

fn make_vote_tx(
    vote_type_script: &packed::Script,
    voter_index: usize,
    direction: u8,
    amount: u64,
) -> TransactionBuilder {
    let voter_args = voter_index.to_le_bytes().to_vec();
    let lock = packed::Script::new_builder()
        .args(packed::Bytes::from(voter_args))
        .build();
    let output = packed::CellOutput::new_builder()
        .lock(lock)
        .type_(
            packed::ScriptOpt::new_builder()
                .set(Some(vote_type_script.clone()))
                .build(),
        )
        .build();
    let vote = Vote::new_builder()
        .vote(direction)
        .amount(uint64(amount))
        .dao_index(Uint16Vec::default())
        .build();
    TransactionBuilder::default()
        .version(uint32(0))
        .output(output)
        .output_data(packed::Bytes::from(vote.as_slice().to_vec()))
}

fn build_block(
    number: u64,
    parent_hash: packed::Byte32,
    txs: Vec<TransactionBuilder>,
) -> BlockView {
    let tx_views: Vec<ckb_types::core::TransactionView> =
        txs.into_iter().map(|tx| tx.build()).collect();
    BlockBuilder::default()
        .number(uint64(number))
        .parent_hash(parent_hash)
        .epoch(epoch())
        .transactions(tx_views)
        .build_unchecked()
}

fn build_voting_chain(
    duration: u32,
    vote_code_hash: Vec<u8>,
    vote_hash_type: u8,
    minimal_requirement: u64,
    num_yes_voters: usize,
    num_no_voters: usize,
) -> (Vec<BlockView>, packed::Script, Proposal) {
    let vote_code_hash_arr: [u8; 32] = vote_code_hash.try_into().unwrap();

    let proposal = Proposal::new_builder()
        .duration(uint32(duration))
        .vote_cell_code_hash(packed::Byte32::from(vote_code_hash_arr))
        .vote_cell_hash_type(vote_hash_type)
        .description(packed::Bytes::default())
        .receiver(packed::Script::default())
        .amount(uint64(0))
        .minimal_requirement(uint64(minimal_requirement))
        .build();

    let proposal_script = make_proposal_script();
    let proposal_blake160: [u8; 20] = {
        let hash = blake2b_256(proposal_script.as_slice());
        let mut b = [0u8; 20];
        b.copy_from_slice(&hash[..20]);
        b
    };
    let vote_type_script = make_vote_type_script(&vote_code_hash_arr, &proposal_blake160);

    let num_blocks = duration as usize + 1;
    let mut blocks = Vec::with_capacity(num_blocks);
    let mut parent_hash = packed::Byte32::zero();

    for i in 0..num_blocks {
        let txs: Vec<TransactionBuilder> = if i == 0 {
            vec![make_proposal_tx(&proposal, &proposal_script)]
        } else {
            let mut txs = Vec::new();
            let voter_base = i - 1;
            for j in 0..num_yes_voters {
                let voter_index = voter_base * 1000 + j;
                txs.push(make_vote_tx(&vote_type_script, voter_index, 1, 100));
            }
            for j in 0..num_no_voters {
                let voter_index = voter_base * 1000 + num_yes_voters + j;
                txs.push(make_vote_tx(&vote_type_script, voter_index, 0, 50));
            }
            txs
        };

        let block = build_block(i as u64, parent_hash, txs);
        parent_hash = block.hash();
        blocks.push(block);
    }

    (blocks, proposal_script, proposal)
}

#[test]
fn test_count_vote_all_yes() {
    let (blocks, proposal_script, proposal) = build_voting_chain(5, vec![1u8; 32], 0, 0, 3, 0);
    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.proposal.as_slice(), proposal.as_slice());
    assert_eq!(result.yes_vote, 5 * 3 * 100);
    assert_eq!(result.no_vote, 0);
    assert!(result.passed);
}

#[test]
fn test_count_vote_all_no() {
    let (blocks, proposal_script, _proposal) = build_voting_chain(5, vec![1u8; 32], 0, 0, 0, 3);
    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.yes_vote, 0);
    assert_eq!(result.no_vote, 5 * 3 * 50);
    assert!(!result.passed);
}

#[test]
fn test_count_vote_mixed() {
    let (blocks, proposal_script, _proposal) = build_voting_chain(5, vec![1u8; 32], 0, 0, 4, 2);
    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.yes_vote, 5 * 4 * 100);
    assert_eq!(result.no_vote, 5 * 2 * 50);
    assert!(result.passed);
}

#[test]
fn test_count_vote_yes_beats_no() {
    let (blocks, proposal_script, _proposal) = build_voting_chain(5, vec![1u8; 32], 0, 0, 1, 2);
    let result = count_vote(&blocks, &proposal_script);
    let expected_yes = 5 * 100;
    let expected_no = 5 * 2 * 50;
    assert_eq!(result.yes_vote, expected_yes);
    assert_eq!(result.no_vote, expected_no);
    assert!(!result.passed);
}

#[test]
fn test_count_vote_minimal_requirement_not_met() {
    let minimal_req = 100_000;
    let (blocks, proposal_script, _proposal) =
        build_voting_chain(5, vec![1u8; 32], 0, minimal_req, 1, 0);
    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.yes_vote, 5 * 100);
    assert_eq!(result.no_vote, 0);
    let min_shannon = minimal_req * 100_000_000;
    assert!(result.yes_vote + result.no_vote <= min_shannon);
    assert!(!result.passed);
}

#[test]
fn test_count_vote_minimal_requirement_met() {
    let (blocks, proposal_script, _proposal) = build_voting_chain(5, vec![1u8; 32], 0, 0, 1, 0);
    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.yes_vote, 5 * 100);
    assert_eq!(result.no_vote, 0);
    assert!(result.passed);
}

#[test]
fn test_count_vote_no_blocks() {
    let blocks: Vec<BlockView> = vec![];
    let proposal_script = make_proposal_script();
    let result = count_vote(&blocks, &proposal_script);
    assert!(!result.passed);
    assert_eq!(result.yes_vote, 0);
    assert_eq!(result.no_vote, 0);
}

#[test]
fn test_vote_retraction_same_voter_overwrites() {
    let (blocks, proposal_script, _proposal) = build_voting_chain(2, vec![1u8; 32], 0, 0, 1, 0);
    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.yes_vote, 2 * 100);
}

#[test]
fn test_dao_double_vote_prevention() {
    let vote_code_hash: [u8; 32] = [1u8; 32];
    let duration = 2u32;

    let proposal = Proposal::new_builder()
        .duration(uint32(duration))
        .vote_cell_code_hash(packed::Byte32::from(vote_code_hash))
        .vote_cell_hash_type(0u8)
        .description(packed::Bytes::default())
        .receiver(packed::Script::default())
        .amount(uint64(0))
        .minimal_requirement(uint64(0))
        .build();

    let proposal_script = make_proposal_script();
    let proposal_blake160: [u8; 20] = {
        let hash = blake2b_256(proposal_script.as_slice());
        let mut b = [0u8; 20];
        b.copy_from_slice(&hash[..20]);
        b
    };
    let vote_type_script = make_vote_type_script(&vote_code_hash, &proposal_blake160);

    let dao_out_point = packed::OutPoint::new_builder()
        .tx_hash(packed::Byte32::from([0x11u8; 32]))
        .index(uint32(0))
        .build();

    let make_vote_with_dao = |voter_index: usize| -> TransactionBuilder {
        let voter_args = voter_index.to_le_bytes().to_vec();
        let lock = packed::Script::new_builder()
            .args(packed::Bytes::from(voter_args))
            .build();

        let cell_dep = packed::CellDep::new_builder()
            .out_point(dao_out_point.clone())
            .dep_type(0u8)
            .build();

        let output = packed::CellOutput::new_builder()
            .lock(lock)
            .type_(
                packed::ScriptOpt::new_builder()
                    .set(Some(vote_type_script.clone()))
                    .build(),
            )
            .build();

        let dao_idx = packed::Uint16::from([0u8, 0u8]);

        let vote = Vote::new_builder()
            .vote(1u8)
            .amount(uint64(100))
            .dao_index(Uint16Vec::new_builder().push(dao_idx).build())
            .build();

        TransactionBuilder::default()
            .version(uint32(0))
            .cell_dep(cell_dep)
            .output(output)
            .output_data(packed::Bytes::from(vote.as_slice().to_vec()))
    };

    let make_spend_tx = || -> TransactionBuilder {
        let input = packed::CellInput::new_builder()
            .previous_output(dao_out_point.clone())
            .since(uint64(0))
            .build();
        TransactionBuilder::default()
            .version(uint32(0))
            .input(input)
    };

    let mut blocks = Vec::with_capacity(3);
    let mut parent_hash = packed::Byte32::zero();

    let block0 = build_block(
        0,
        parent_hash,
        vec![make_proposal_tx(&proposal, &proposal_script)],
    );
    parent_hash = block0.hash();
    blocks.push(block0);

    let block1 = build_block(1, parent_hash, vec![make_vote_with_dao(1)]);
    parent_hash = block1.hash();
    blocks.push(block1);

    let block2 = build_block(2, parent_hash, vec![make_spend_tx()]);
    blocks.push(block2);

    let result = count_vote(&blocks, &proposal_script);
    assert_eq!(result.yes_vote, 0);
}
