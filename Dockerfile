# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.100.0
ARG DEBIAN_RELEASE=trixie

# Toolchain plus sources; shared by the test and build stages.
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS source
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Formatting, lints and tests: docker build --target test .
FROM source AS test
RUN rustup component add rustfmt clippy
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo fmt --check \
    && cargo clippy --locked --release --all-targets -- -D warnings \
    && cargo test --locked --release

FROM source AS build
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --locked --release \
    && cp target/release/poker /usr/local/bin/poker

FROM debian:${DEBIAN_RELEASE}-slim AS runtime
RUN useradd --create-home --uid 10001 poker \
    && install -d -o poker -g poker /data
COPY --from=build /usr/local/bin/poker /usr/local/bin/poker
USER poker
# Hand histories are written here; mount a volume to keep them.
WORKDIR /data
VOLUME /data
ENTRYPOINT ["poker"]
