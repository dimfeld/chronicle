FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
ENV SQLX_OFFLINE=true
ENV SQLX_OFFLINE_DIR=/app/.sqlx
RUN apt-get update && apt-get install -y pkg-config libssl-dev
WORKDIR /app

FROM chef AS planner
COPY api/ .
ENV SQLX_OFFLINE=true
# Make sure sqlx always looks in this directory, even when building other crates.
ENV SQLX_OFFLINE_DIR=/app/.sqlx
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS api-builder
COPY --from=planner /app/recipe.json recipe.json
COPY api/.sqlx ./.sqlx/
RUN cargo chef cook --release --recipe-path recipe.json
COPY api/ .
RUN cargo build --release --bin chronicle-api

##### Final image
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y dumb-init pkg-config libssl-dev ca-certificates && apt-get clean
RUN update-ca-certificates
WORKDIR /app
ARG TARGETARCH

RUN mkdir -p /data
COPY --from=api-builder /app/target/release/chronicle-api /app/chronicle-api

ENV HOST=::0
ENV ENV=production
ENV INSECURE=false

# Primary server port
ENV PORT=8443
EXPOSE 8443/tcp
ENV API_SERVER=localhost:8443

ENV ALLOW_PUBLIC_SIGNUP=false
ENV ALLOW_INVITE_TO_SAME_ORG=false
ENV ALLOW_INVITE_TO_NEW_ORG=false

# If the application is using an Effectum queue, place it here.
ENV QUEUE_PATH=/data/queue.db

# [dumb-init](https://github.com/Yelp/dumb-init) for proper PID1 handling
ENTRYPOINT [ "/app/chronicle-api" ]
