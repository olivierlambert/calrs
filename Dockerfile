# Stage 1: Build
FROM rust:slim-trixie AS builder

WORKDIR /build

# Pre-build dependencies (cached unless Cargo.toml/Cargo.lock change)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

# Build the real binary
COPY src/ src/
COPY migrations/ migrations/
COPY assets/ assets/
RUN touch src/main.rs && cargo build --release

# Stage 2: Runtime
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false -m -d /var/lib/calrs calrs

COPY --from=builder /build/target/release/calrs /usr/local/bin/calrs
COPY templates/ /opt/calrs/templates/

WORKDIR /opt/calrs
USER calrs

ENV CALRS_DATA_DIR=/var/lib/calrs
EXPOSE 3000

ENTRYPOINT ["calrs"]
CMD ["serve", "--host", "0.0.0.0", "--port", "3000"]
