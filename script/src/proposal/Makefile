SCHEMA_DIR := ../../../util/gen-types/schemas

generate: types.rs
	@rustfmt types.rs
	@sed -i '' 's|^use super::protocols::\*;|use ckb_types::packed::*;\nuse ckb_types::molecule;\nuse ckb_types::molecule::prelude::*;|' types.rs
	@sed -i '' '/^use molecule::prelude::\*;$$/d' types.rs
	@rm -f blockchain.mol extensions.mol protocols.mol

types.rs: types.mol
	@set -eu; \
	cp $(SCHEMA_DIR)/blockchain.mol .; \
	cp $(SCHEMA_DIR)/extensions.mol .; \
	cp $(SCHEMA_DIR)/protocols.mol .; \
	moleculec --language rust --schema-file types.mol > types.rs
	@perl -i -pe '$$_ .= "#![allow(missing_docs)]\n#![allow(clippy::all)]\n" if $$. == 1' types.rs

clean:
	rm -f blockchain.mol extensions.mol protocols.mol

.PHONY: generate clean
