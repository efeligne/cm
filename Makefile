.PHONY: release
release:
	CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
	CARGO_PROFILE_RELEASE_LTO=fat \
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=abort \
	CARGO_PROFILE_RELEASE_STRIP=symbols \
	RUSTFLAGS="-C target-cpu=native" \
	cargo build --release --locked

.PHONY: patch
patch:
	cargo release patch --execute

.PHONY: minor
minor:
	cargo release minor --execute

.PHONY: major
major:
	cargo release major --execute
