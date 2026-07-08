
## Scope
The rules in this document only apply to the submodule `proposal` of ckb-script crate, located at ./script.

## Document
The proposal spec is located at ../ckb-treasury-lab/docs/proposal-type-script.md
Follow the spec when making changes. If some rules are not covered by the spec, you can use your own judgment.

## Tests
After every change, test it with:
```
cargo test -- proposal
```

## Clippy and fmt
After batch changes, run these commands once:
```
cd ..
make clippy
make fmt
make check-whitespaces
```
Do not run them after every small change.

## Cycles
When adding code whose computation scales with input data, charge cycles accordingly.


## Panic free
It should not panic. Propagate all errors, especially note math operations (e.g., add, sub, mul).

