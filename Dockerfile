# syntax=docker/dockerfile:1

################################################################################
# Build the server and the WASM client.
# trixie: the wasm-bindgen binary downloaded by trunk needs glibc 2.38 or newer.
FROM rust:1.99-slim-trixie AS build

ARG TRUNK_VERSION=v0.22.1

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rustup target add wasm32-unknown-unknown \
    && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/${TRUNK_VERSION}/trunk-$(uname -m)-unknown-linux-musl.tar.gz" \
        | tar -xz -C /usr/local/bin

WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY public ./public

RUN cargo build --release --locked -p tracker-server
RUN cd crates/client && trunk build --release --locked

################################################################################
# Run them with minimal runtime dependencies.
FROM debian:trixie-slim AS final

WORKDIR /usr/src/app
COPY --from=build /usr/src/app/target/release/tracker-server ./tracker-server
COPY --from=build /usr/src/app/dist ./dist
# The server runs as nobody and some folders are private to their owner.
COPY --chown=nobody:nogroup public ./public

ENV BIND=0.0.0.0:3000
ENV PUBLIC_DIR=/usr/src/app/public
ENV WEB_DIR=/usr/src/app/dist

# Run the application as a non-root user.
USER nobody

EXPOSE 3000

CMD ["./tracker-server"]
