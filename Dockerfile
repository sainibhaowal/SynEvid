# syntax=docker/dockerfile:1
# Production Multi-Arch Dockerfile for SynEvid (Code Autopsy)
# Image: ghcr.io/sainibhaowal/synevid

FROM rust:1.90-alpine AS builder

RUN apk add --no-cache musl-dev perl make gcc g++ git bash

WORKDIR /build
COPY . .

# Build release binaries with optimized size
RUN cargo build --release -p autopsy-cli -p synevid-lsp && \
    strip target/release/autopsy && \
    strip target/release/synevid-lsp

# Minimal secure runtime image
FROM alpine:3.20 AS runtime

RUN apk add --no-cache ca-certificates git bash

# Non-root user for sandboxed security (INV-SANDBOX-001)
RUN addgroup -S -g 10001 synevid && \
    adduser -S -u 10001 -G synevid synevid

COPY --from=builder /build/target/release/autopsy /usr/local/bin/autopsy
COPY --from=builder /build/target/release/synevid-lsp /usr/local/bin/synevid-lsp

RUN ln -s /usr/local/bin/autopsy /usr/local/bin/synevid && \
    ln -s /usr/local/bin/autopsy /usr/local/bin/synevid-ui && \
    chmod +x /usr/local/bin/autopsy /usr/local/bin/synevid-lsp

USER synevid:synevid
WORKDIR /workspace

EXPOSE 3333

ENTRYPOINT ["/usr/local/bin/autopsy"]
CMD ["--help"]
