use super::super::block_verifier::{
    BlockBytesVerifier, BlockProposalsLimitVerifier, CellbaseVerifier, DuplicateVerifier,
    MerkleRootVerifier,
};
use crate::{BlockErrorKind, CellbaseError};
use ckb_error::assert_error_eq;
use ckb_types::{
    bytes::Bytes,
    core::{
        capacity_bytes, BlockBuilder, BlockNumber, Capacity, HeaderBuilder, TransactionBuilder,
        TransactionView,
    },
    h256,
    packed::{Byte32, CellInput, CellOutputBuilder, OutPoint, ProposalShortId, Script},
    prelude::*,
    H256,
};

fn create_cellbase_transaction_with_block_number(number: BlockNumber) -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new_cellbase_input(number))
        .output(
            CellOutputBuilder::default()
                .capacity(capacity_bytes!(100).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .witness(Script::default().into_witness())
        .build()
}

fn create_cellbase_transaction_with_capacity(capacity: Capacity) -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new_cellbase_input(0))
        .output(
            CellOutputBuilder::default()
                .capacity(capacity.pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .witness(Script::default().into_witness())
        .build()
}

fn create_cellbase_transaction_with_non_empty_output_data() -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new_cellbase_input(0))
        .output(
            CellOutputBuilder::default()
                .capacity(capacity_bytes!(100).pack())
                .build(),
        )
        .output_data(Bytes::from("123").pack())
        .witness(Script::default().into_witness())
        .build()
}

fn create_cellbase_transaction_with_two_output() -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new_cellbase_input(0))
        .output(
            CellOutputBuilder::default()
                .capacity(capacity_bytes!(100).pack())
                .build(),
        )
        .output(
            CellOutputBuilder::default()
                .capacity(capacity_bytes!(100).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .witness(Script::default().into_witness())
        .build()
}

fn create_cellbase_transaction_with_two_output_data() -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new_cellbase_input(0))
        .output(
            CellOutputBuilder::default()
                .capacity(capacity_bytes!(100).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .output_data(Bytes::new().pack())
        .witness(Script::default().into_witness())
        .build()
}

fn create_cellbase_transaction() -> TransactionView {
    create_cellbase_transaction_with_capacity(capacity_bytes!(100))
}

fn create_normal_transaction() -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new(OutPoint::new(h256!("0x1").pack(), 0), 0))
        .output(
            CellOutputBuilder::default()
                .capacity(capacity_bytes!(100).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build()
}

#[test]
pub fn test_block_without_cellbase() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(1u64.pack()).build())
        .transaction(TransactionBuilder::default().build())
        .build();
    let verifier = CellbaseVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        CellbaseError::InvalidQuantity,
    );
}

#[test]
pub fn test_block_with_one_cellbase_at_first() {
    let transaction = create_normal_transaction();

    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(1u64.pack()).build())
        .transaction(create_cellbase_transaction_with_block_number(1))
        .transaction(transaction)
        .build();

    let verifier = CellbaseVerifier::new();
    assert!(verifier.verify(&block).is_ok());
}

#[test]
pub fn test_block_with_correct_cellbase_number() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_cellbase_transaction_with_block_number(2))
        .build();

    let verifier = CellbaseVerifier::new();
    assert!(verifier.verify(&block).is_ok());
}

#[test]
pub fn test_block_with_incorrect_cellbase_number() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_cellbase_transaction_with_block_number(3))
        .build();

    let verifier = CellbaseVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        CellbaseError::InvalidInput,
    );
}

#[test]
pub fn test_block_with_one_cellbase_at_last() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_normal_transaction())
        .transaction(create_cellbase_transaction())
        .build();

    let verifier = CellbaseVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        CellbaseError::InvalidPosition,
    );
}

#[test]
pub fn test_cellbase_with_non_empty_output_data() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_cellbase_transaction_with_non_empty_output_data())
        .build();
    let verifier = CellbaseVerifier::new();
    assert_eq!(
        verifier.verify(&block),
        Err(VerifyError::Cellbase(CellbaseError::InvalidOutputData))
    )
}

#[test]
pub fn test_cellbase_with_two_output() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_cellbase_transaction_with_two_output())
        .build();
    let verifier = CellbaseVerifier::new();
    assert_eq!(
        verifier.verify(&block),
        Err(VerifyError::Cellbase(CellbaseError::InvalidQuantity))
    )
}

#[test]
pub fn test_cellbase_with_two_output_data() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_cellbase_transaction_with_two_output_data())
        .build();
    let verifier = CellbaseVerifier::new();
    assert_eq!(
        verifier.verify(&block),
        Err(VerifyError::Cellbase(CellbaseError::InvalidQuantity))
    )
}

#[test]
pub fn test_block_with_duplicated_txs() {
    let tx = create_normal_transaction();
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(tx.clone())
        .transaction(tx)
        .build();

    let verifier = DuplicateVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        BlockErrorKind::DuplicatedCommittedTransactions,
    );
}

#[test]
pub fn test_block_with_duplicated_proposals() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .proposal(ProposalShortId::zero())
        .proposal(ProposalShortId::zero())
        .build();

    let verifier = DuplicateVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        BlockErrorKind::DuplicatedProposalTransactions,
    );
}

#[test]
pub fn test_transaction_root() {
    let header = HeaderBuilder::default()
        .number(2u64.pack())
        .transactions_root(Byte32::zero())
        .build();
    let block = BlockBuilder::default()
        .header(header)
        .transaction(create_normal_transaction())
        .build_unchecked();

    let verifier = MerkleRootVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        BlockErrorKind::UnmatchedCommittedRoot,
    );
}

#[test]
pub fn test_proposals_root() {
    let header = HeaderBuilder::default()
        .number(2u64.pack())
        .proposals_hash(h256!("0x1").pack())
        .build();
    let block = BlockBuilder::default()
        .header(header)
        .transaction(create_normal_transaction())
        .build_unchecked();

    let verifier = MerkleRootVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        BlockErrorKind::UnmatchedCommittedRoot,
    );
}

#[test]
pub fn test_witnesses_root() {
    let header = HeaderBuilder::default()
        .number(2u64.pack())
        .witnesses_root(h256!("0x1").pack())
        .build();
    let block = BlockBuilder::default()
        .header(header)
        .proposal(ProposalShortId::zero())
        .build_unchecked();

    let verifier = MerkleRootVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        BlockErrorKind::UnmatchedWitnessesRoot,
    );
}

#[test]
pub fn test_block_with_two_cellbases() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .transaction(create_cellbase_transaction())
        .transaction(create_cellbase_transaction())
        .build();

    let verifier = CellbaseVerifier::new();
    assert_error_eq(
        verifier.verify(&block).unwrap_err(),
        CellbaseError::InvalidQuantity,
    );
}

#[test]
pub fn test_cellbase_with_less_reward() {
    let transaction = create_normal_transaction();

    let block = BlockBuilder::default()
        .transaction(create_cellbase_transaction_with_capacity(capacity_bytes!(
            50
        )))
        .transaction(transaction)
        .build();

    let verifier = CellbaseVerifier::new();
    assert!(verifier.verify(&block).is_ok());
}

#[test]
pub fn test_cellbase_with_fee() {
    let transaction = create_normal_transaction();

    let block = BlockBuilder::default()
        .transaction(create_cellbase_transaction_with_capacity(capacity_bytes!(
            110
        )))
        .transaction(transaction)
        .build();

    let verifier = CellbaseVerifier::new();
    assert!(verifier.verify(&block).is_ok());
}

#[test]
pub fn test_max_block_bytes_verifier_skip_genesis() {
    let block = BlockBuilder::default().build();
    {
        let verifier = BlockBytesVerifier::new(block.serialized_size() as u64);
        assert!(verifier.verify(&block).is_ok());
    }

    {
        let verifier = BlockBytesVerifier::new(block.serialized_size() as u64 - 1);
        assert!(verifier.verify(&block).is_ok());
    }
}

#[test]
pub fn test_max_block_bytes_verifier() {
    let block = BlockBuilder::default()
        .header(HeaderBuilder::default().number(2u64.pack()).build())
        .build();

    {
        let verifier = BlockBytesVerifier::new(block.serialized_size() as u64);
        assert!(verifier.verify(&block).is_ok());
    }

    {
        let verifier = BlockBytesVerifier::new(block.serialized_size() as u64 - 1);
        assert_error_eq(
            verifier.verify(&block).unwrap_err(),
            BlockErrorKind::TooLargeSize,
        );
    }
}

#[test]
pub fn test_max_proposals_limit_verifier() {
    let block = BlockBuilder::default()
        .proposal(ProposalShortId::zero())
        .build();

    {
        let verifier = BlockProposalsLimitVerifier::new(1);
        assert!(verifier.verify(&block).is_ok());
    }

    {
        let verifier = BlockProposalsLimitVerifier::new(0);
        assert_error_eq(
            verifier.verify(&block).unwrap_err(),
            BlockErrorKind::TooManyProposals,
        );
    }
}
