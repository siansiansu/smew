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

# smew shells out to `aws ssm start-session`, which in turn drives
# session-manager-plugin (see src/session/driver.rs). Both come from the
# official AWS installers; TARGETARCH picks the right one per platform.
#
# Versions are pinned and sha256-verified. To bump: raise the two versions,
# then refresh all four checksums, e.g.
#   curl -fsSL <url> | sha256sum
# with the URLs below (x86_64/aarch64 zip, ubuntu_64bit/ubuntu_arm64 deb).
ARG AWSCLI_VERSION=2.36.18
ARG AWSCLI_SHA256_X86_64=5243d4ced9ce3d0864f04fb3b5609f5b6ce57bb2d9ae3ef5eee80a3548b41505
ARG AWSCLI_SHA256_AARCH64=8259d6dcede6812f19b1fe88a16ebdf62580ad95ff1d6caec9c58af56543d8e0
ARG SMP_VERSION=1.2.835.0
ARG SMP_SHA256_64BIT=7c6dcad12518571cc7959a713e6a8ae1bdf6ed66fd9bee37dc189e39ca58ae03
ARG SMP_SHA256_ARM64=0add94c4c8b6ca63f26e44fd655d662b0f6455a268b5b9ebebee0f462214e928
ARG TARGETARCH
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl unzip \
    && case "$TARGETARCH" in \
         amd64) awsarch=x86_64;  awssha=$AWSCLI_SHA256_X86_64; \
                smparch=64bit;   smpsha=$SMP_SHA256_64BIT ;; \
         arm64) awsarch=aarch64; awssha=$AWSCLI_SHA256_AARCH64; \
                smparch=arm64;   smpsha=$SMP_SHA256_ARM64 ;; \
         *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
       esac \
    && curl -fsSL "https://awscli.amazonaws.com/awscli-exe-linux-${awsarch}-${AWSCLI_VERSION}.zip" -o /tmp/awscliv2.zip \
    && echo "${awssha}  /tmp/awscliv2.zip" | sha256sum -c - \
    && unzip -q /tmp/awscliv2.zip -d /tmp \
    && /tmp/aws/install \
    && curl -fsSL "https://s3.amazonaws.com/session-manager-downloads/plugin/${SMP_VERSION}/ubuntu_${smparch}/session-manager-plugin.deb" -o /tmp/session-manager-plugin.deb \
    && echo "${smpsha}  /tmp/session-manager-plugin.deb" | sha256sum -c - \
    && dpkg -i /tmp/session-manager-plugin.deb \
    && apt-get purge -y curl unzip \
    && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/* /tmp/aws /tmp/awscliv2.zip /tmp/session-manager-plugin.deb

COPY --from=builder /src/target/release/smew /usr/local/bin/smew

RUN useradd --create-home --user-group smew
USER smew
WORKDIR /home/smew

# Interactive TUI: give the panes a capable terminal by default.
ENV TERM=xterm-256color

ENTRYPOINT ["smew"]
