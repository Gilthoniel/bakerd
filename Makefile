setup-tools:
	cargo install cargo-tarpaulin

test:
	cargo tarpaulin --ignore-tests --out Html

migration-redo:
	diesel migration redo --database-url data.db

install:
	cargo build -r
	cp target/release/bakerd bakerd/usr/bin/.
	dpkg-deb --root-owner-group --build bakerd
