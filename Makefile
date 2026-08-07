# smew (version in Cargo.toml).

build:
	cargo build --release

test:
	cargo test

docker-build:
	docker build -t smew .

# Interactive TUI: mounts ~/.aws read-only for credentials/profiles and
# passes AWS_PROFILE / AWS_REGION through when set.
docker-run:
	docker run -it --rm \
		-v $$HOME/.aws:/home/smew/.aws:ro \
		-e AWS_PROFILE -e AWS_REGION \
		smew

.PHONY: build test docker-build docker-run
