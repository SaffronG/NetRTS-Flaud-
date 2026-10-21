# Build stage
FROM rust:1-bookworm AS build
WORKDIR /app

# Cache dependencies separately from the source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/NetRTS-Flaud- /usr/local/bin/netrts-flaud

ENV NETRTS_SERVER=https://netrts.snowse.io

ENTRYPOINT ["netrts-flaud"]
