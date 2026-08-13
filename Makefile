# smew (version in Cargo.toml).

help:
	@echo "Available targets:"
	@echo "  build         Build release binary"
	@echo "  test          Run tests"
	@echo "  fmt           Format code"
	@echo "  lint          Run clippy lints"
	@echo "  check         Full pre-commit gate: lint, test, fmt --check"
	@echo "  dev           Run TUI against built-in mock account"
	@echo "  docker-build  Build docker image"
	@echo "  docker-run    Run TUI in docker"
	@echo "  install       Install smew to ~/.cargo/bin via 'cargo install'"
	@echo "  uninstall     Remove smew installed via 'cargo install'"

build:
	cargo build --release

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets -- -D warnings

# The full pre-commit gate: format check, lints, tests.
check: lint test
	cargo fmt --check

# Run the TUI against the built-in mock account (no AWS access needed).
dev:
	cargo run -- --dev

docker-build:
	docker build -t smew .

# Interactive TUI: mounts ~/.aws read-only for credentials/profiles and
# passes AWS_PROFILE / AWS_REGION through when set.
docker-run:
	docker run -it --rm \
		-v $$HOME/.aws:/home/smew/.aws:ro \
		-e AWS_PROFILE -e AWS_REGION \
		smew

install:
	cargo install --path .

uninstall:
	cargo uninstall smew

.PHONY: help build test fmt lint check dev docker-build docker-run install uninstall
