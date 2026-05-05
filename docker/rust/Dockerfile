FROM ghcr.io/jamietre/crabbit-base:latest

RUN apt-get update -qq && apt-get install -y build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --no-modify-path

ENV PATH="/root/.cargo/bin:${PATH}"
