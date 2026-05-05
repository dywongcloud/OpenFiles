FROM rust:1.86-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p openfiles-cli -p openfiles-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates fuse3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/openfiles /usr/local/bin/openfiles
COPY --from=builder /src/target/release/openfiles-server /usr/local/bin/openfiles-server
ENTRYPOINT ["openfiles-server"]
