.PHONY: build test lint fmt clippy check clean doc release
.PHONY: python-build python-test python-lint python-fmt
.PHONY: docker-build docker-test cross-install cross-build cross-test ci

# PyO3 0.23 caps at Python 3.13; set this so builds succeed with newer interpreters.
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY := 1

# === Local targets ===
build:
	cargo build --workspace

test:
	cargo test --workspace

lint: fmt clippy

fmt:
	cargo fmt --check

clippy:
	cargo clippy --workspace -- -D warnings

check:
	cargo check --workspace

doc:
	cargo doc --workspace --no-deps

clean:
	cargo clean

release:
	cargo build --workspace --release

# === Docker cross-platform test (Linux) ===
docker-build:
	docker build -t agentdir-test .

docker-test: docker-build
	docker run --rm agentdir-test

# === Cross-platform testing (Windows via cross + Wine) ===
# Requires: cargo install cross --git https://github.com/cross-rs/cross
# Note: cross uses Docker internally. Requires x86_64 host (ARM64/Apple Silicon
# may fail due to QEMU+Wine limitations — see cross-rs/cross#1372).
# Wine does NOT support CoW reflinks — reflink tests will exercise the byte-copy
# fallback path, which is the correct NTFS behavior anyway.
# File watcher tests (notify crate) may not work under Wine; they are skipped
# on Windows targets via #[cfg] gates if needed.

cross-install:
	cargo install cross --git https://github.com/cross-rs/cross

cross-build:
	@echo "Checking Windows cross-compilation..."
	cross build --workspace --target x86_64-pc-windows-gnu

cross-test:
	@echo "Running Windows tests via cross + Wine..."
	cross test --workspace --target x86_64-pc-windows-gnu

# === Python targets ===
python-build:
	cd bindings/python && uv run maturin develop

python-test:
	cd bindings/python && uv run pytest -v

python-lint:
	cd bindings/python && uv run ruff check . && uv run ruff format --check . && uv run deptry .

python-fmt:
	cd bindings/python && uv run ruff format .

# === Full CI-equivalent ===
ci: fmt clippy test doc python-lint python-test
