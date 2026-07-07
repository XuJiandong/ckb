

Based on the spec in ~/projects/ckb-treasury-lab/docs/proposal-type-script.md, implement a proposal type script.
It can be implemented in ckb-script(./script), in folder ./script/src/proposal. 


There is a reference implement under folder: `~/projects/ckb-vote-poc/crates/verification`, function `ckb_vote_verification::count_vote`.

Make sure this module can be mocked. It will be used to do unit tests. Loading blocks should be mocked. Add test case
based on `~projects/ckb-vote-poc/crates/verification/benches/count_vote.rs`

