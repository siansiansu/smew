# syntax=docker/dockerfile:1

# ---- builder ----
# Rust 1.90 covers edition 2024 (Cargo.toml) and matches the toolchain the
# project is developed with.
FROM rust:1.90-slim-bookworm AS builder

# aws-lc-sys (TLS backend of the AWS SDK) compiles C sources with cmake.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build --release --locked

# ---- runtime ----
FROM debian:bookworm-slim

# Sessions run through session-manager-plugin (smew makes the StartSession
# API call itself — no aws CLI needed; see src/session/ssm.rs). The plugin
# comes from the official AWS installer; TARGETARCH picks the right one.
#
# The version is pinned and sha256-verified. To bump: raise the version,
# then refresh both checksums, e.g.
#   curl -fsSL <url> | sha256sum
# with the URL below (ubuntu_64bit/ubuntu_arm64 deb).
ARG SMP_VERSION=1.2.835.0
ARG SMP_SHA256_64BIT=7c6dcad12518571cc7959a713e6a8ae1bdf6ed66fd9bee37dc189e39ca58ae03
ARG SMP_SHA256_ARM64=0add94c4c8b6ca63f26e44fd655d662b0f6455a268b5b9ebebee0f462214e928
ARG TARGETARCH
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && case "$TARGETARCH" in \
         amd64) smparch=64bit;  smpsha=$SMP_SHA256_64BIT ;; \
         arm64) smparch=arm64;  smpsha=$SMP_SHA256_ARM64 ;; \
         *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
       esac \
    && curl -fsSL "https://s3.amazonaws.com/session-manager-downloads/plugin/${SMP_VERSION}/ubuntu_${smparch}/session-manager-plugin.deb" -o /tmp/session-manager-plugin.deb \
    && echo "${smpsha}  /tmp/session-manager-plugin.deb" | sha256sum -c - \
    && dpkg -i /tmp/session-manager-plugin.deb \
    && apt-get purge -y curl \
    && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/* /tmp/session-manager-plugin.deb

COPY --from=builder /src/target/release/smew /usr/local/bin/smew

RUN useradd --create-home --user-group smew
USER smew
WORKDIR /home/smew

# Interactive TUI: give the panes a capable terminal by default.
ENV TERM=xterm-256color

ENTRYPOINT ["smew"]
