# ==============================================================================
# 1. BUILD STAGE
# ==============================================================================
FROM ubuntu:24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

# Install compilation toolchain & libraries
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    cmake \
    clang \
    libclang-dev \
    llvm-dev \
    pkg-config \
    libssl-dev \
    libfontconfig1-dev \
    libfreetype6-dev \
    git \
    ca-certificates \
    tar \
    && rm -rf /var/lib/apt/lists/*

# Install official Microsoft ONNX Runtime 1.24.4
ARG ONNX_VERSION=1.24.4
RUN mkdir -p /opt/onnxruntime && \
    curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ONNX_VERSION}/onnxruntime-linux-x64-${ONNX_VERSION}.tgz" | \
    tar -xz --strip-components=1 -C /opt/onnxruntime

# Link against the official v1.24.4 shared library
ENV ORT_STRATEGY=system \
    ORT_LIB_LOCATION=/opt/onnxruntime/lib \
    ORT_PREFER_DYNAMIC_LINK=1 \
    LD_LIBRARY_PATH="/opt/onnxruntime/lib:${LD_LIBRARY_PATH}"

# Install Rust toolchain (stable)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /usr/src/catsbot

# Copy manifests, assets, and source tree
COPY Cargo.toml Cargo.lock* ./
COPY assets ./assets
COPY src ./src

# Build release binary using Podman / Buildah cache mounts
# Target cache keeps dependency .rlib/.so artifacts intact between runs
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/usr/src/catsbot/target \
    cargo build --release && \
    cp /usr/src/catsbot/target/release/catsbot /usr/src/catsbot/catsbot-bin

# ==============================================================================
# 2. RUNTIME STAGE
# ==============================================================================
FROM ubuntu:24.04 AS runner

ENV DEBIAN_FRONTEND=noninteractive

# Install shared runtime libraries
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libfontconfig1 \
    libfreetype6 \
    libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the v1.24.4 ONNX Runtime libraries into runtime image
COPY --from=builder /opt/onnxruntime/lib/libonnxruntime.so* /usr/lib/

# Copy compiled binary (from the copied artifact)
COPY --from=builder /usr/src/catsbot/catsbot-bin /app/catsbot

# Set default application environment variables
ENV MORPHOLOGY_DB=assets/text/morphology.db \
    WORDS_DB=assets/text/words.db \
    SENTENCES_DB=assets/text/sentences.db \
    MODEL_DIR=assets/robert-tiny2 \
    APP_DB=app_database \
    IMAGE_CACHE_DIR=.image_cache \
    RUST_LOG=info

ENTRYPOINT ["/app/catsbot"]
