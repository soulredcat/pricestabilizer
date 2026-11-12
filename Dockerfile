# Build:  docker build -t pricestabilizer .
# Run:    docker run --rm -v "$PWD/config:/config:ro" -v "$PWD/data:/data" pricestabilizer
# Check:  docker run --rm -v "$PWD/config:/config:ro" pricestabilizer check-config /config/engine.toml
#
# A venue connector for your market is not part of this image: build your
# own image FROM this one and add the connector, then point
# [venue] command at it.

FROM rust:1.99-slim-trixie AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:trixie-slim
RUN useradd --system --uid 10001 --home-dir /data engine \
    && mkdir -p /data /config \
    && chown engine /data
COPY --from=build /build/target/release/pricestabilizer /usr/local/bin/pricestabilizer
COPY config /usr/share/pricestabilizer/config
USER engine
WORKDIR /data
ENTRYPOINT ["pricestabilizer"]
CMD ["/config/engine.toml"]
