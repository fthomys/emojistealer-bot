FROM rust:1-alpine AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev gcc
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM alpine:3 AS runtime
RUN adduser -D -H -u 65532 bot
WORKDIR /app
COPY --from=builder /app/target/release/stealemoji-bot /app/stealemoji-bot
USER bot
ENTRYPOINT ["/app/stealemoji-bot"]
