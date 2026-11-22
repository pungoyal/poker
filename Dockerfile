# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.100.0
ARG DEBIAN_RELEASE=trixie

# Toolchain plus sources; shared by the test and build stages.
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS source
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web

# Formatting, lints and tests: docker build --target test .
FROM source AS test
RUN rustup component add rustfmt clippy
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo fmt --all --check \
    && cargo clippy --locked --release --all-targets -- -D warnings \
    && cargo test --locked --release

# The engine compiled to WebAssembly, next to the page's static files.
FROM source AS build
# Must match the wasm-bindgen version in web/Cargo.toml.
ARG WASM_BINDGEN_VERSION=0.2.130
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} --locked
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    ./web/build.sh && cp -r web/www /site

# A static site: nothing of the game runs on the server.
FROM nginxinc/nginx-unprivileged:alpine AS runtime
COPY --from=build /site /usr/share/nginx/html
EXPOSE 8080
