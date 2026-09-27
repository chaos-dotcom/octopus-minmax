# Build stage: compile the Rust implementation.
FROM rust:1.98-slim AS build

ARG BOT_VERSION=v.local
ENV OCTO_BOT_VERSION=${BOT_VERSION}

WORKDIR /build
COPY rust/Cargo.toml rust/Cargo.lock /build/rust/
COPY rust/src /build/rust/src
RUN cargo build --release --manifest-path /build/rust/Cargo.toml

# Runtime stage: a static-ish binary, no interpreter, no dependency tree.
FROM debian:bookworm-slim

WORKDIR /app
COPY --from=build /build/rust/target/release/octo-minmax /app/octo-minmax
RUN mkdir -p /app/logs
EXPOSE 5050

CMD ["/app/octo-minmax"]
