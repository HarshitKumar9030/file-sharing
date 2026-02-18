# Stage 1: Build the application
FROM rust:1-slim-bullseye as builder

WORKDIR /usr/src/app

# Copy manifest and source
COPY Cargo.toml Cargo.lock ./
# Create a dummy main.rs to build dependencies first (caching layer)
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Now copy the real source code and static assets
COPY src ./src
COPY static ./static

# Touch main.rs to force rebuild of the application code
RUN touch src/main.rs
RUN cargo build --release

# Stage 2: Runtime environment
FROM debian:bullseye-slim

WORKDIR /app

# Install runtime dependencies (OpenSSL is often required by Actix/Rust network apps)
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/app/target/release/file-sharing /app/file-sharing
RUN chmod +x /app/file-sharing

# Create uploads directory
RUN mkdir -p /app/uploads

# Expose port
EXPOSE 8080

# Run the application
# Use environment variable for bind address if needed, but 0.0.0.0 is good for docker
ENV BIND_ADDR=0.0.0.0:8080
CMD ["./file-sharing"]
