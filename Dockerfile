# Build stage
FROM rust:latest as builder

# Install OpenSSL development packages
RUN apt-get update && apt-get install -y libssl-dev pkg-config && rm -rf /var/lib/apt/lists/*

# Set the working directory in the container
WORKDIR /usr/src/near-indexer

# Copy the Cargo.toml and Cargo.lock files
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src ./src

# Build the application
RUN cargo build --release

# Final stage
FROM ubuntu:22.04

# Install OpenSSL and ca-certificates
RUN apt-get update && apt-get install -y openssl ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/near-indexer/target/release/near-indexer /usr/local/bin/near-indexer

# Set the working directory
WORKDIR /usr/local/bin

# Run the binary
CMD ["near-indexer"]