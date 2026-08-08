# smew (version in Cargo.toml).

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

.PHONY: build test fmt lint check dev docker-build docker-run
