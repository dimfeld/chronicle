FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
ENV SQLX_OFFLINE=true
ENV SQLX_OFFLINE_DIR=/app/.sqlx
RUN apt-get update && apt-get install -y pkg-config libssl-dev
WORKDIR /app

FROM chef AS planner
COPY . .
ENV SQLX_OFFLINE=true
# Make sure sqlx always looks in this directory, even when building other crates.
ENV SQLX_OFFLINE_DIR=/app/api/.sqlx
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS api-builder
COPY --from=planner /app/recipe.json recipe.json
COPY api/.sqlx ./api/.sqlx/
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin chronicle

##### Final image
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y pkg-config libssl-dev ca-certificates && apt-get clean
RUN update-ca-certificates
WORKDIR /app
ARG TARGETARCH

RUN mkdir -p /data
COPY --from=api-builder /app/target/release/chronicle /app/chronicle

ENV HOST=::0
ENV ENV=production

# Primary server port
ENV PORT=9782
EXPOSE 9782/tcp

ENTRYPOINT [ "/app/chronicle" ]
CMD [ "serve" ]
