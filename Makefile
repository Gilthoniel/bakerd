setup-tools:
	cargo install cargo-tarpaulin

test:
	cargo tarpaulin --ignore-tests --out Html

migration-redo:
	diesel migration redo --database-url data.db
