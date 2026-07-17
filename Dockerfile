# syntax=docker/dockerfile:1
#
# Production OCI for the Intel OpenVINO edge runtime.
# Builds `dg-cli` with the `product-intel` feature set and packages it into a
# non-root Ubuntu 24.04 x86_64 image with the OpenVINO 2026.2.1 runtime and
# software FFmpeg codec libraries.

ARG BASE_IMAGE=ubuntu@sha256:52df9b1ee71626e0088f7d400d5c6b5f7bb916f8f0c82b474289a4ece6cf3faf
ARG RUST_VERSION=1.94.1
ARG OPENVINO_VERSION=2026.2.1

# -----------------------------------------------------------------------------
# Builder stage
# -----------------------------------------------------------------------------
FROM ${BASE_IMAGE} AS builder

ARG RUST_VERSION
ENV DEBIAN_FRONTEND=noninteractive
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH="${CARGO_HOME}/bin:${PATH}"

# Build dependencies: Rust toolchain, Python/pip (for OpenVINO runtime libs),
# clang/libclang (bindgen), FFmpeg development libraries.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    build-essential \
    pkg-config \
    python3 \
    python3-pip \
    python3-venv \
    clang \
    libclang-dev \
    cmake \
    ninja-build \
    nasm \
    libavutil-dev \
    libavcodec-dev \
    libavformat-dev \
    libswscale-dev \
    libswresample-dev \
    && rm -rf /var/lib/apt/lists/*

# Install a pinned Rust toolchain matching the workspace rust-version.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain "${RUST_VERSION}" \
    --profile minimal

WORKDIR /workspace
COPY . .

# Configure the bindgen / software-avcodec environment, then build the
# product-intel `dg-cli` binary. Cargo cache mounts are used to speed up
# rebuilds and keep the image layer small.
RUN --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/target \
    bash -c ' \
      source scripts/env-software-avcodec.sh && \
      cargo build --release --locked -p dg-cli \
        --no-default-features --features product-intel \
    ' && \
    cp /workspace/target/release/dg-cli /tmp/dg-cli

# -----------------------------------------------------------------------------
# Runtime stage
# -----------------------------------------------------------------------------
FROM ${BASE_IMAGE} AS runtime

ARG OPENVINO_VERSION
ENV DEBIAN_FRONTEND=noninteractive

# Runtime dependencies: Python/pip for OpenVINO shared libraries and the
# FFmpeg runtime libraries required by the software H.264 profile.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    python3 \
    python3-pip \
    python3-venv \
    libavutil58 \
    libavcodec60 \
    libavformat60 \
    libswscale7 \
    libswresample4 \
    && rm -rf /var/lib/apt/lists/*

ARG OPENVINO_VERSION
RUN python3 -m pip install --no-cache-dir --break-system-packages \
    "openvino==${OPENVINO_VERSION}" \
    || python3 -m pip install --no-cache-dir "openvino==${OPENVINO_VERSION}"

# Expose OpenVINO libraries through a stable path independent of the exact
# Python site-packages directory. This path is used by LD_LIBRARY_PATH and by
# the runtime loader.
RUN OV_LIBS=$(python3 -c \
    'import openvino, pathlib; print((pathlib.Path(openvino.__file__).resolve().parent / "libs").resolve())') && \
    mkdir -p /opt/openvino && ln -sfn "${OV_LIBS}" /opt/openvino/lib

# Non-root user with groups that can access /dev/dri for Intel iGPU. The host
# must map the device and may need `--group-add $(getent group render | cut -d: -f3)`.
# Ubuntu 24.04 already reserves UID 1000 (`ubuntu`) so we pick 1001.
RUN (getent group render || groupadd -r render) && \
    (getent group video || groupadd -r video) && \
    useradd -m -u 1001 -G render,video dyun

# Runtime directories for configuration, state and models.
RUN mkdir -p /etc/dyun /var/lib/dyun /models && \
    chown -R dyun:dyun /etc/dyun /var/lib/dyun /models

COPY --from=builder /tmp/dg-cli /usr/local/bin/dg
RUN chmod +x /usr/local/bin/dg

# The OpenVINO C API and its plugin libraries are loaded from /opt/openvino/lib.
# FFmpeg libraries are available in the standard system library path.
ENV LD_LIBRARY_PATH=/opt/openvino/lib
ENV LIBYUV_TARGET=ubuntu-24.04_x86_64

USER dyun
EXPOSE 9090

VOLUME ["/etc/dyun", "/var/lib/dyun", "/models"]

ENTRYPOINT ["/usr/local/bin/dg"]
# Bind the ops/metrics server to all container interfaces so EXPOSE 9090 is usable.
# Host networking security (auth, TLS, firewall) must be configured separately.
CMD ["run", "--config", "/etc/dyun/graph.yaml", "--ops-bind", "0.0.0.0:9090"]
