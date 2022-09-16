setup-tools:
	rustup component add llvm-tools-preview
	cargo install grcov

test:
	rm -rf ./target/report

	@RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="target/report/coverage-report.profraw" cargo test

	@grcov . \
		-s . \
		--ignore build.rs \
		--ignore "/*" \
		--ignore "target/debug/*" \
		--binary-path ./target/debug \
		-t html \
		--branch \
		--ignore-not-existing \
		--excl-br-start "mod tests \{" \
		--excl-start "mod tests \{" \
		-o ./target/report \
		./target/report/coverage-report.profraw

test-ci:
	@RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="target/report/coverage-report.profraw" cargo test

	@grcov . \
		-s . \
		--ignore build.rs \
		--ignore "/*" \
		--ignore "target/debug/*" \
		--binary-path ./target/debug/deps \
		-t lcov \
		--branch \
		--ignore-not-existing \
		--excl-br-start "mod tests \{" \
		--excl-start "mod tests \{" \
		-o ./target/report/lcov.info \
		./target/report/coverage-report.profraw

migration-redo:
	diesel migration redo --database-url data.db

install:
	cargo build -r
	cp target/release/bakerd bakerd/usr/bin/.
	dpkg-deb --root-owner-group --build bakerd
