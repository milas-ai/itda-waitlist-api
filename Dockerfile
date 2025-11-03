# Build stage
FROM rust:1-bookworm AS builder

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

# Final stage
FROM debian:bookworm-slim
ARG DEFAULT_PORT=8286

RUN apt-get update \
    && apt-get install -y ca-certificates \ 
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/itad-waitlist-api usr/local/bin/itad-waitlist-api

ENV PORT=${DEFAULT_PORT}
EXPOSE ${DEFAULT_PORT}

CMD ["itad-waitlist-api"]
